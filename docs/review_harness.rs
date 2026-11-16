// Review harness: exercises MicroParquet's calculation code against hand-checked cases.
// Each test prints what the code produces so findings can be cited with evidence.
use mp_harness::banking::*;
use mp_harness::engine::*;
use mp_harness::models::*;
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

fn dtype_of(cfg: &IngestionConfig, col: &str) -> ColDataType {
    cfg.columns.iter().find(|c| c.name == col).unwrap().dtype
}

#[test]
fn t01_schema_inference() {
    let p = write_csv(
        "infer.csv",
        "Actnum,Account_Balance,Residual_Maturity,Reporting_Date,Matdat,Opndat_US,Balance,Liquidity_Bucket_Amt,Paid_Amount\n\
         0012345,1500000.50,15/11/2026,31/10/2026,05/11/2026,10/31/2026,100,5,7\n\
         0012346,2500000.00,20/12/2027,31/10/2026,15/03/2027,11/30/2026,200,6,8\n\
         0012347,(350.25),01/01/2030,31/10/2026,28/02/2028,12/31/2026,300,7,9\n",
    );
    let cfg = auto_detect_smart_schema(&p).unwrap();
    for c in &cfg.columns {
        println!("INFER {:<22} -> {:?}", c.name, c.dtype);
    }
}

#[test]
fn t02_ambiguous_dates_prefer_mdy() {
    // All days <= 12: genuinely ambiguous; Bangladesh CBS exports are usually D/M/Y
    let p = write_csv(
        "ambig.csv",
        "Matdat\n3/4/2027\n5/6/2027\n11/12/2027\n",
    );
    let cfg = auto_detect_smart_schema(&p).unwrap();
    println!("AMBIG Matdat -> {:?}", dtype_of(&cfg, "Matdat"));
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
    println!("DATES DMY parse:\n{}", df);
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
    println!("NUMS:\n{}", df);
    let (rows, sum) = compute_control_totals(lf, Some("Balance")).unwrap();
    println!("NUMS control rows={} sum={:?}", rows, sum);
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
    let p = tmpdir().join("dt.parquet");
    let mut f = std::fs::File::create(&p).unwrap();
    ParquetWriter::new(&mut f).finish(&mut df).unwrap();

    let lf = build_lazy_pipeline(&p, true, &IngestionConfig::default(), &tmpdir()).unwrap();
    let preset = &CustomBucketDef::default_presets()[0];
    let out = apply_maturity_bucket(lf.clone(), "Matdat", "2026-10-31", preset, true, "Bucket")
        .unwrap()
        .collect();
    match out {
        Ok(d) => println!("PARQ-DATETIME bucket (datetime col):\n{}", d.select(["Matdat", "Bucket"]).unwrap()),
        Err(e) => println!("PARQ-DATETIME bucket error: {e}"),
    }
    // string-typed date column (common when parquet came from a CSV with no typing)
    let out2 = apply_maturity_bucket(lf, "Matdat_str", "2026-10-31", preset, true, "Bucket")
        .unwrap()
        .collect();
    match out2 {
        Ok(d) => println!("PARQ-STRING bucket (string col):\n{}", d.select(["Matdat_str", "Bucket"]).unwrap()),
        Err(e) => println!("PARQ-STRING bucket error: {e}"),
    }
}

#[test]
fn t06_auto_format_classification() {
    for name in [
        "Modified_Duration",
        "Macaulay_Duration_Yrs",
        "Corporate_Loan_Balance",
        "Account_Balance",
        "Discount_Amount",
        "Rate_Delta_Bps",
        "Utility_Bill_Amount",
        "Balance",
        "No_of_Accounts",
    ] {
        let s = format_metric_value(25_000_000.0, name, None, NumberDisplayFormat::BdtCrore, 2);
        println!("FMT {:<24} 25,000,000 -> {}", name, s);
    }
}

#[test]
fn t07_provisioning() {
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
    println!("PROV:\n{}", out.drop("Balance").unwrap());
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
    let step = TransformStep::GroupByPivot {
        group_cols: vec!["Grp".into()],
        pivot_col: None,
        aggs: vec![AggSpec {
            target_col: "Rate".into(),
            func: AggFunc::WeightedAvg,
            weight_col: Some("Balance".into()),
            alias: "WAVG_Rate".into(),
            format_spec: ColumnFormatSpec::default(),
        }],
        sort_by_col: None,
        top_n: None,
    };
    // compile via a parquet source so the real step path is exercised
    let p = tmpdir().join("wavg.parquet");
    let mut d = df.clone();
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut d).unwrap();
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    let out = compile_tab_lazy(&src, &[step], &tmpdir()).unwrap().collect().unwrap();
    println!("WAVG (expected 10.0):\n{}", out);
}

#[test]
fn t09_duration_and_eve() {
    // 5y fixed 10% annual-pay par instrument, BDT 100: textbook Macaulay D = 4.1699, ModD = 3.7908
    let cutoff = "2026-10-31";
    let mat = chrono::NaiveDate::from_ymd_opt(2031, 10, 30).unwrap(); // 1825 days later
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let mat_days = (mat - epoch).num_days() as i32;
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
    println!("DURATION:\n{}", out);
    // Exact full-reprice ΔEVE for +200bp on the asset: P(12%) - 100
    let p12: f64 = (1..=5).map(|t| 10.0 / 1.12f64.powi(t)).sum::<f64>() + 100.0 / 1.12f64.powi(5);
    println!("EXACT dEVE asset +200bp = {:.4}  (first-order estimate shown above)", p12 - 100.0);
}

#[test]
fn t10_nii_shock_signs() {
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let d = |y, m, dd| (chrono::NaiveDate::from_ymd_opt(y, m, dd).unwrap() - epoch).num_days() as i32;
    let df = df!(
        "Side" => ["Loan 3m", "Term deposit 3m", "Savings (no maturity)"],
        "Balance" => [1_000_000.0, 1_000_000.0, 1_000_000.0],
        "Rate" => [12.0, 9.0, 4.0],
        "Matdat" => [Some(d(2027, 1, 29)), Some(d(2027, 1, 29)), None]
    )
    .unwrap()
    .lazy()
    .with_column(col("Matdat").cast(DataType::Date));
    let out = apply_rate_shock_sim(df, "Balance", "Rate", "Matdat", "2026-10-31", 100.0)
        .unwrap()
        .collect()
        .unwrap();
    println!("NII SHOCK +100bp:\n{}", out);
}

#[test]
fn t11_ftp_step_vs_interpolation() {
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let o = (chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() - epoch).num_days() as i32;
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
    println!("FTP (curve 91d=9.50, 182d=10.10, 30d=8.75):\n{}", out);
}

#[test]
fn t12_reconcile() {
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
    println!("RECONCILE (A1 prev WAVG rate = 9.5%):\n{}", out);
}

#[test]
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
    let p = tmpdir().join("ids.parquet");
    let mut d = df.clone();
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut d).unwrap();
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    let step = TransformStep::FilterRows {
        column: "Actnum".into(),
        operator: FilterOp::Eq,
        value: "12345678901234567".into(),
        keep_matching: true,
    };
    let out = compile_tab_lazy(&src, &[step], &tmpdir()).unwrap().collect().unwrap();
    println!("FILTER Actnum == 12345678901234567 returned {} rows:\n{}", out.height(), out);
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
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Balance".into(),
            dtype: ColDataType::Float64,
            pii_mask: PiiMaskMode::AccountNumber,
            sample_values: vec![],
        }],
        ..Default::default()
    };
    let py = mp_harness::macros::generate_python_polars_code(
        std::path::Path::new("x.csv"),
        false,
        &cfg,
        &steps,
        None,
    );
    println!("PYTHON:\n{}", py);
}

#[test]
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
fn t17_join_duplicates_inflate() {
    let left = df!("Actnum" => ["A1"], "Balance" => [1_000_000.0]).unwrap();
    let right = df!("Actnum" => ["A1", "A1"], "Sector" => ["RMG", "RMG (dup row)"]).unwrap();
    let pl = tmpdir().join("l.parquet");
    let pr = tmpdir().join("r.parquet");
    ParquetWriter::new(std::fs::File::create(&pl).unwrap()).finish(&mut left.clone()).unwrap();
    ParquetWriter::new(std::fs::File::create(&pr).unwrap()).finish(&mut right.clone()).unwrap();
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
}

#[test]
fn t18_segment_and_status_edge_cases() {
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
    println!("PROV-EDGE:\n{}", out.select(["CL", "Seg", "NPL_Category", "Statutory_Prov_Rate_Pct"]).unwrap());
}

#[test]
fn t19_hygiene_case_sensitivity() {
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let past = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() - epoch).num_days() as i32;
    let open = (chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() - epoch).num_days() as i32;
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
    println!("HYGIENE:\n{}", out.select(["Acstat", "Audit_Exception_Flag"]).unwrap());
}
