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
            ..Default::default()
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
fn t03b_date_formats_phase2() {
    // P3: month names, "." separators and two-digit years (00-69 -> 20xx, 70-99 -> 19xx).
    let d = |raw: &str, mode| parse_flexible_banking_date(raw, mode);
    use ColDataType::*;
    assert_eq!(d("5/11/26", DateDMY), Some(days(2026, 11, 5)));
    assert_eq!(d("11/5/26", DateMDY), Some(days(2026, 11, 5)));
    assert_eq!(d("31-OCT-2026", DateDMY), Some(days(2026, 10, 31)));
    assert_eq!(d("31-Oct-26", DateDMY), Some(days(2026, 10, 31)));
    assert_eq!(d("31-oct-26", DateMDY), Some(days(2026, 10, 31)));
    assert_eq!(d("05.11.2026", DateDMY), Some(days(2026, 11, 5)));
    assert_eq!(d("01/01/75", DateDMY), Some(days(1975, 1, 1)));
    assert_eq!(d("01/01/69", DateDMY), Some(days(2069, 1, 1)));
    assert_eq!(d("01/01/70", DateDMY), Some(days(1970, 1, 1)));
    // A year that is neither two nor four digits is not a date.
    for bad in ["5/11/026", "5/11/-5", "5/11/2O26", "31/02/2026", "5/11"] {
        assert_eq!(d(bad, DateDMY), None, "{bad}");
    }

    // The same values through a CSV load.
    let p = write_csv(
        "dates_p3.csv",
        "Matdat\n5/11/26\n31-OCT-2026\n31-Oct-26\n05.11.2026\n01/01/75\n",
    );
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig { name: "Matdat".into(), dtype: DateDMY, ..Default::default() }],
        ..Default::default()
    };
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(
        date_days_col(&df, "Matdat"),
        vec![
            Some(days(2026, 11, 5)),
            Some(days(2026, 10, 31)),
            Some(days(2026, 10, 31)),
            Some(days(2026, 11, 5)),
            Some(days(1975, 1, 1)),
        ]
    );
}

#[test]
fn t03c_blank_date_column_and_hygiene_audit() {
    // P3: a column blank in every sampled row is a date when its name says so.
    let p = write_csv(
        "blank_close.csv",
        "Opndat,Matdat,Closure_Date,Remarks,Acstat,Balance,Rate\n\
         01/01/2024,01/01/2025,,,ACT,500,9\n\
         01/06/2024,01/01/2027,,,ACT,700,9\n",
    );
    let cfg = auto_detect_smart_schema(&p).unwrap();
    assert_eq!(dtype_of(&cfg, "Closure_Date"), ColDataType::DateDMY);
    assert_eq!(dtype_of(&cfg, "Remarks"), ColDataType::Utf8);
    let lf = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap();
    let out = apply_banking_hygiene_audit(
        lf, "Opndat", "Matdat", "Closure_Date", "Acstat", "Balance", "Rate", "2026-10-31", false,
        &default_active_statuses(), 0.0, default_rate_max_pct(),
    )
    .unwrap()
    .collect()
    .unwrap();
    let flags = str_col(&out, "Audit_Exception_Flag");
    assert_eq!(flags.len(), 2);
    assert_eq!(flags[0].as_deref(), Some("1. GHOST_MATURED_ACTIVE"));
}

#[test]
fn t04a_numeric_parsing_phase1() {
    // Phase 1 behaviour: comma thousands and bracket negatives parse; row count is kept.
    // (Dr/Cr rows are covered by t04: without a sign convention they now stop the load.)
    let p = write_csv(
        "nums_p1.csv",
        "Balance\n\"1,234.50\"\n(500.00)\n1234.50\n-250.00\n-\n 99.5 \nBDT 10\n",
    );
    let cfg = IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Balance".into(),
            dtype: ColDataType::Float64,
            pii_mask: PiiMaskMode::None,
            sample_values: vec![],
            ..Default::default()
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
fn t04_numeric_parsing_silent_nulls() {
    // No value silently becomes null: Dr/Cr per the column's sign convention, "-" = 0,
    // "BDT 10" = 10. Parsed control sum = 1,828.50 with 0 nulls.
    let p = write_csv(
        "nums.csv",
        "Balance\n\"1,234.50\"\n(500.00)\n1234.50 Cr\n250.00 Dr\n-\n 99.5 \nBDT 10\n",
    );
    let cfg_with = |sign| IngestionConfig {
        columns: vec![ColumnSchemaConfig {
            name: "Balance".into(),
            dtype: ColDataType::Float64,
            sign_convention: sign,
            ..Default::default()
        }],
        ..Default::default()
    };
    let lf = build_lazy_pipeline(&p, false, &cfg_with(SignConvention::CrPositive), &tmpdir()).unwrap();
    let vals = f64_col(&lf.clone().collect().unwrap(), "Balance");
    assert_eq!(
        vals,
        vec![Some(1234.5), Some(-500.0), Some(1234.5), Some(-250.0), Some(0.0), Some(99.5), Some(10.0)]
    );
    let (rows, sum) = compute_control_totals(lf, Some("Balance")).unwrap();
    assert_eq!(rows, 7);
    assert_close(sum.unwrap(), 1828.5, "parsed control sum");

    // Dr is +: the Dr/Cr rows flip, everything else is unchanged.
    let lf = build_lazy_pipeline(&p, false, &cfg_with(SignConvention::DrPositive), &tmpdir()).unwrap();
    assert_eq!(
        f64_col(&lf.collect().unwrap(), "Balance"),
        vec![Some(1234.5), Some(-500.0), Some(-1234.5), Some(250.0), Some(0.0), Some(99.5), Some(10.0)]
    );

    // Sign convention not set: loading stops and names the column.
    let err = build_lazy_pipeline(&p, false, &cfg_with(SignConvention::Unset), &tmpdir())
        .err()
        .expect("Dr/Cr amounts with no sign convention must not load");
    let msg = format!("{err:#}");
    assert!(msg.contains("'Balance'") && msg.contains("Dr/Cr"), "{msg}");

    // Auto-detect marks the column so the schema modal can highlight it.
    let auto = auto_detect_smart_schema(&p).unwrap();
    let bal = auto.columns.iter().find(|c| c.name == "Balance").unwrap();
    assert_eq!(bal.dtype, ColDataType::Float64);
    assert!(bal.has_dr_cr);
    assert_eq!(bal.sign_convention, SignConvention::Unset);
}

#[test]
fn t04b_amount_parsing_rules() {
    use SignConvention::*;
    let v = |raw: &str| parse_banking_amount(raw, CrPositive, true);
    assert_eq!(v("500-"), AmountCell::Value(-500.0));
    assert_eq!(v("Tk. 1,000"), AmountCell::Value(1000.0));
    assert_eq!(v("tk1,000"), AmountCell::Value(1000.0));
    assert_eq!(v("৳ 75"), AmountCell::Value(75.0));
    assert_eq!(v("BDT(1,000.50)"), AmountCell::Value(-1000.5));
    assert_eq!(v("1,000 dr."), AmountCell::Value(-1000.0));
    assert_eq!(v("CR 20"), AmountCell::Value(20.0));
    assert_eq!(v("Tk 5 Dr"), AmountCell::Value(-5.0));
    assert_eq!(v("-"), AmountCell::Value(0.0));
    for blank in ["", "  ", "NA", "n/a", "NULL"] {
        assert_eq!(v(blank), AmountCell::Blank, "{blank:?}");
    }
    for bad in ["abc", "1.2.3", "1e5", "inf", "NaN", "(500) Dr", "--5", "Cr"] {
        assert_eq!(v(bad), AmountCell::Invalid, "{bad:?}");
    }
    assert_eq!(parse_banking_amount("5 Dr", Unset, true), AmountCell::NeedsSignConvention);
    // Accounting negatives switched off: brackets and a trailing minus are not numbers.
    assert_eq!(parse_banking_amount("(5)", CrPositive, false), AmountCell::Invalid);
    assert_eq!(parse_banking_amount("5-", CrPositive, false), AmountCell::Invalid);
    assert_eq!(parse_banking_amount("1,234", CrPositive, false), AmountCell::Value(1234.0));
}

#[test]
fn t22_parse_failure_counts() {
    // P2: non-blank values that became null are counted per typed column, with examples.
    let p = write_csv(
        "pf.csv",
        "Balance,Matdat,Name\n100,05/11/2026,a\nabc,31/02/2026,b\n200,,c\n,-,d\n",
    );
    let cfg = IngestionConfig {
        columns: vec![
            ColumnSchemaConfig { name: "Balance".into(), dtype: ColDataType::Float64, ..Default::default() },
            ColumnSchemaConfig { name: "Matdat".into(), dtype: ColDataType::DateDMY, ..Default::default() },
            ColumnSchemaConfig { name: "Name".into(), dtype: ColDataType::Utf8, ..Default::default() },
        ],
        ..Default::default()
    };
    let f = compute_parse_failures(&p, false, &cfg, &tmpdir()).unwrap();
    assert_eq!(
        f,
        vec![
            ParseFailure { column: "Balance".into(), count: 1, examples: vec!["abc".into()] },
            ParseFailure { column: "Matdat".into(), count: 1, examples: vec!["31/02/2026".into()] },
        ]
    );
    assert_eq!(parse_failure_summary(&f), "Parse failures: Balance 1, Matdat 1");
    assert_eq!(parse_failure_summary(&[]), "");

    // A stack of the same file twice adds the counts up per column.
    let stack = TabSource::VerticalStack {
        files: vec![(p.clone(), false, cfg.clone()), (p.clone(), false, cfg.clone())],
        include_source_col: false,
    };
    let sf = source_parse_failures(&stack, &tmpdir()).unwrap();
    assert_eq!(sf[0].count, 2);
    assert_eq!(sf[0].examples, vec!["abc".to_string(), "abc".to_string()]);

    // "Export anyway" list: <output>.parse_failures.csv next to the export.
    let out = tmpdir().join("pf_out.csv");
    let side = write_parse_failures_csv(&out, &f).unwrap();
    assert_eq!(side, tmpdir().join("pf_out.csv.parse_failures.csv"));
    assert_eq!(
        std::fs::read_to_string(&side).unwrap(),
        "column,count,examples\n\"Balance\",1,\"abc\"\n\"Matdat\",1,\"31/02/2026\"\n"
    );
}

#[test]
fn t22b_headless_stops_on_control_column_failures() {
    // Auto-detect would type this column as text ("abc"), so the schema comes from a macro.
    let p = write_csv("pf_headless.csv", "Balance\n100\nabc\n200\n\n");
    let recipe = MacroRecipe {
        name: "pf".into(),
        description: String::new(),
        created_at: String::new(),
        enforce_balance_control: true,
        control_column: Some("Balance".into()),
        optional_ingestion_schema: Some(IngestionConfig {
            columns: vec![ColumnSchemaConfig {
                name: "Balance".into(),
                dtype: ColDataType::Float64,
                ..Default::default()
            }],
            ..Default::default()
        }),
        steps: vec![],
    };
    let m = tmpdir().join("pf_headless.mpq.json");
    std::fs::write(&m, serde_json::to_string(&recipe).unwrap()).unwrap();
    let out = tmpdir().join("pf_headless_out.csv");
    let _ = std::fs::remove_file(&out);
    let run = |extra: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_MicroParquet"))
            .args(["--headless", "--input"])
            .arg(&p)
            .arg("--macro-file")
            .arg(&m)
            .arg("--export-csv")
            .arg(&out)
            .args(extra)
            .output()
            .unwrap()
    };
    let r = run(&[]);
    let err = String::from_utf8_lossy(&r.stderr).to_string();
    assert!(!r.status.success(), "must exit non-zero: {err}");
    assert!(err.contains("Parse failures: Balance 1"), "{err}");
    assert!(err.contains("--allow-parse-failures"), "{err}");
    assert!(!out.exists(), "nothing is exported when the run stops");

    let r = run(&["--allow-parse-failures"]);
    let err = String::from_utf8_lossy(&r.stderr).to_string();
    assert!(r.status.success(), "{err}");
    assert!(err.contains("Parse failures: Balance 1"), "{err}");
    assert!(out.exists());
}

fn write_bytes(name: &str, body: &[u8]) -> PathBuf {
    let p = tmpdir().join(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn t23_csv_reading_options() {
    // P4: pipe separator, auto-detected from the header.
    let p = write_csv("pipe.csv", "Actnum|Balance|Matdat\n0012345|\"1,000.50\"|05/11/2026\n0012346|200|06/11/2026\n");
    let cfg = auto_detect_smart_schema(&p).unwrap();
    assert_eq!(cfg.separator, '|');
    let names: Vec<&str> = cfg.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Actnum", "Balance", "Matdat"]);
    assert_eq!(dtype_of(&cfg, "Balance"), ColDataType::Float64);
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&df, "Actnum")[0].as_deref(), Some("0012345"));
    assert_eq!(f64_col(&df, "Balance"), vec![Some(1000.5), Some(200.0)]);
    assert_eq!(detect_separator("a;b;c"), ';');
    assert_eq!(detect_separator("a\tb\tc"), '\t');
    assert_eq!(detect_separator("\"x,y\"|b|c"), '|', "commas inside quotes do not count");
    assert_eq!(detect_separator("single"), ',');

    // Three title lines above the header.
    let body = "Bank Ltd - Loan book\nAs of 31/10/2026, BDT\n\nActnum,Balance\n001,100\n002,200\n";
    let p = write_csv("titles.csv", body);
    let base = IngestionConfig { skip_rows: 3, ..Default::default() };
    let cfg = auto_detect_smart_schema_with(&p, &base).unwrap();
    assert_eq!(cfg.skip_rows, 3);
    let names: Vec<&str> = cfg.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Actnum", "Balance"]);
    let lf = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap();
    let (rows, sum) = compute_control_totals(lf, Some("Balance")).unwrap();
    assert_eq!((rows, sum), (2, Some(300.0)));
    // The line healer honours skip_rows and the separator too.
    let p = write_csv("titles_pipe.csv", "Title\nA|B|C\n1|multi\nline|5\n2|z|6\n");
    let cfg = IngestionConfig {
        skip_rows: 1,
        separator: '|',
        heal_broken_csv_lines: true,
        columns: vec![
            ColumnSchemaConfig { name: "A".into(), dtype: ColDataType::Int64, ..Default::default() },
            ColumnSchemaConfig { name: "B".into(), ..Default::default() },
            ColumnSchemaConfig { name: "C".into(), ..Default::default() },
        ],
        ..Default::default()
    };
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&df, "B"), vec![Some("multi line".to_string()), Some("z".to_string())]);
    assert_eq!(f64_col(&df, "A"), vec![Some(1.0), Some(2.0)]);

    // Windows-1252: é is 0xE9, € is 0x80.
    let p = write_bytes("cp1252.csv", b"Name,Amount\nCaf\xe9,100\n\x80 Bond,200\n");
    let cfg = auto_detect_smart_schema(&p).unwrap();
    assert_eq!(cfg.encoding, CsvTextEncoding::Windows1252);
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&df, "Name"), vec![Some("Café".to_string()), Some("€ Bond".to_string())]);
    assert_eq!(f64_col(&df, "Amount"), vec![Some(100.0), Some(200.0)]);
    assert_eq!(decode_windows_1252(b"\x93x\x94 \x96 \xa0"), "\u{201c}x\u{201d} \u{2013} \u{a0}");

    // A UTF-8 BOM never stays on the first column name.
    let p = write_bytes("bom.csv", b"\xef\xbb\xbfBalance,Name\n1,a\n2,b\n");
    let cfg = auto_detect_smart_schema(&p).unwrap();
    assert_eq!(cfg.columns[0].name, "Balance");
    assert_eq!(preview_csv_raw(&p, 5).unwrap().get_column_names()[0].as_str(), "Balance");
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&df, "Balance"), vec![Some(1.0), Some(2.0)]);
    // Same when the user loads the file with no columns configured.
    let raw = build_lazy_pipeline(&p, false, &IngestionConfig::default(), &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(raw.get_column_names()[0].as_str(), "Balance");
}

#[test]
fn t24_parquet_schema() {
    // P5: String columns of a Parquet file convert with the CSV rules; masks apply.
    let df = df!(
        "Actnum" => ["0012345678", "0098765432", "0011112222"],
        "Balance" => ["1,000", "(50)", "abc"],
        "Matdat" => ["05/11/2026", "31-Oct-26", ""],
        "Rate" => [9.5, 10.0, 11.0],
        "Flag" => [true, false, true]
    )
    .unwrap();
    let p = write_parquet("p5.parquet", &df);

    let mut cfg = auto_detect_parquet_schema(&p).unwrap();
    let labels: Vec<(&str, &str, ColDataType)> = cfg
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.source_dtype.as_str(), c.dtype))
        .collect();
    assert_eq!(
        labels,
        vec![
            ("Actnum", "String", ColDataType::Utf8),
            ("Balance", "String", ColDataType::Utf8),
            ("Matdat", "String", ColDataType::Utf8),
            ("Rate", "Float64", ColDataType::Float64),
            ("Flag", "Boolean", ColDataType::Utf8),
        ]
    );
    assert_eq!(cfg.columns[1].sample_values, vec!["1,000", "(50)", "abc"]);

    // The auto-detected schema changes nothing.
    let same = build_lazy_pipeline(&p, true, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert!(same.equals_missing(&df));

    let set = |cfg: &mut IngestionConfig, name: &str, f: &dyn Fn(&mut ColumnSchemaConfig)| {
        f(cfg.columns.iter_mut().find(|c| c.name == name).unwrap())
    };
    set(&mut cfg, "Balance", &|c| c.dtype = ColDataType::Float64);
    set(&mut cfg, "Matdat", &|c| c.dtype = ColDataType::DateDMY);
    set(&mut cfg, "Actnum", &|c| c.pii_mask = PiiMaskMode::AccountNumber);
    let out = build_lazy_pipeline(&p, true, &cfg, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&out, "Balance"), vec![Some(1000.0), Some(-50.0), None]);
    assert_eq!(
        date_days_col(&out, "Matdat"),
        vec![Some(days(2026, 11, 5)), Some(days(2026, 10, 31)), None]
    );
    let acct = str_col(&out, "Actnum");
    let a0 = acct[0].clone().unwrap();
    assert!(a0.starts_with("ACCT_") && a0.len() == 21 && !a0.contains("001"), "ACCT_ + 16 hex: {a0}");
    assert_eq!(f64_col(&out, "Rate"), vec![Some(9.5), Some(10.0), Some(11.0)]);
    assert_eq!(out.column("Flag").unwrap().dtype(), &DataType::Boolean);

    // P2 counts cover Parquet conversions ("abc"; the blank date is not a failure).
    let f = compute_parse_failures(&p, true, &cfg, &tmpdir()).unwrap();
    assert_eq!(f, vec![ParseFailure { column: "Balance".into(), count: 1, examples: vec!["abc".into()] }]);

    // A numeric Parquet column changed to Text becomes text; a Boolean cannot become a number.
    let mut c2 = cfg.clone();
    set(&mut c2, "Rate", &|c| c.dtype = ColDataType::Utf8);
    let out = build_lazy_pipeline(&p, true, &c2, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&out, "Rate")[0].as_deref(), Some("9.5"));
    set(&mut c2, "Flag", &|c| c.dtype = ColDataType::Float64);
    let err = build_lazy_pipeline(&p, true, &c2, &tmpdir()).err().expect("Boolean -> Float64 must fail");
    assert!(format!("{err:#}").contains("'Flag' is Boolean"), "{err:#}");

    // Dr/Cr in a Parquet String column with no sign convention stops the load too.
    let p2 = write_parquet("p5_drcr.parquet", &df!("Balance" => ["100 Dr", "50 Cr"]).unwrap());
    let mut c3 = auto_detect_parquet_schema(&p2).unwrap();
    c3.columns[0].dtype = ColDataType::Float64;
    let err = build_lazy_pipeline(&p2, true, &c3, &tmpdir()).err().expect("Dr/Cr with no sign must fail");
    assert!(format!("{err:#}").contains("'Balance' has Dr/Cr"), "{err:#}");
    c3.columns[0].sign_convention = SignConvention::DrPositive;
    let out = build_lazy_pipeline(&p2, true, &c3, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&out, "Balance"), vec![Some(100.0), Some(-50.0)]);
}

#[test]
fn t24b_headless_parquet_applies_macro_schema() {
    // Macro schema on a Parquet input: String columns that exist are converted, a native
    // column keeps its type, and absent columns are skipped with a warning on stderr.
    let p = write_parquet(
        "p5_headless.parquet",
        &df!(
            "Balance" => ["1,000", "(50)"],
            "Matdat" => ["05/11/2026", "31-Oct-26"],
            "Rate" => [9.5, 10.0]
        )
        .unwrap(),
    );
    let col = |name: &str, dtype| ColumnSchemaConfig { name: name.into(), dtype, ..Default::default() };
    let saved = IngestionConfig {
        columns: vec![
            col("Balance", ColDataType::Float64),
            col("Matdat", ColDataType::DateDMY),
            col("Rate", ColDataType::Utf8),
            col("Branch_Name", ColDataType::Utf8),
            col("Old_Limit", ColDataType::Float64),
        ],
        ..Default::default()
    };
    let (fitted, missing) = adapt_schema_to_parquet(&p, &saved).unwrap();
    assert_eq!(missing, vec!["Branch_Name".to_string(), "Old_Limit".to_string()]);
    let kept: Vec<&str> = fitted.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(kept, vec!["Balance", "Matdat"]);

    let recipe = MacroRecipe {
        name: "pq".into(),
        description: String::new(),
        created_at: String::new(),
        enforce_balance_control: true,
        control_column: Some("Balance".into()),
        optional_ingestion_schema: Some(saved),
        steps: vec![],
    };
    let m = tmpdir().join("p5_headless.mpq.json");
    std::fs::write(&m, serde_json::to_string(&recipe).unwrap()).unwrap();
    let out = tmpdir().join("p5_headless_out.parquet");
    let _ = std::fs::remove_file(&out);
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_MicroParquet"))
        .args(["--headless", "--input"])
        .arg(&p)
        .arg("--macro-file")
        .arg(&m)
        .arg("--export-parquet")
        .arg(&out)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&r.stderr).to_string();
    assert!(r.status.success(), "{err}");
    assert!(err.contains("not in the Parquet file, skipped: Branch_Name, Old_Limit"), "{err}");
    let df = LazyFrame::scan_parquet(&out, ScanArgsParquet::default()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&df, "Balance"), vec![Some(1000.0), Some(-50.0)]);
    assert_eq!(date_days_col(&df, "Matdat"), vec![Some(days(2026, 11, 5)), Some(days(2026, 10, 31))]);
    assert_eq!(df.column("Rate").unwrap().dtype(), &DataType::Float64, "native column keeps its type");
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
    let e2 = apply_ftp_curve_map(df.clone().lazy(), "Opndat", "Matdat", "Balance", "Rate", &default_bd_ftp_curve(), true, FtpInterpolation::Linear).err();
    let e3 = apply_banking_hygiene_audit(df.clone().lazy(), "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false, &default_active_statuses(), 0.0, 25.0).err();
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
    assert_eq!(src[6].as_deref(), Some("BRPD Circular Letter No. 29, 21 Dec 2025"), "checked v3 row: no [verify]");
    assert!(src.iter().all(|s| !s.as_deref().unwrap().contains("[verify]")), "{src:?}");
    // A row still flagged verify (v2 defaults) gets the " [verify]" suffix
    let v2 = ProvisionConfig { rules: ProvisionConfig::default_rules_for(2), ..ProvisionConfig::defaults() };
    let out2 = apply_loan_classification_provision_with(df.slice(6, 1).lazy(), "Balance", "CL", Some("Seg"), Some("Susp"), Some("Sec"), None, "2026-10-31", &v2)
        .unwrap()
        .collect()
        .unwrap();
    assert!(str_col(&out2, "Rule_Source")[0].as_deref().unwrap().ends_with("[verify]"));
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
    let out = apply_ftp_curve_map(df, "Opndat", "Matdat", "Balance", "Rate", &default_bd_ftp_curve(), true, FtpInterpolation::Linear)
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(f64_col(&out, "Tenor_Days"), vec![Some(100.0), Some(31.0)]);
}

#[test]
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
    let run = |lf: LazyFrame, curve: &[FtpPoint], m: FtpInterpolation| {
        apply_ftp_curve_map(lf, "Opndat", "Matdat", "Balance", "Rate", curve, true, m).unwrap().collect().unwrap()
    };
    let out = run(df.clone(), &default_bd_ftp_curve(), FtpInterpolation::Linear);
    let ftp = f64_col(&out, "Benchmark_FTP_Rate");
    assert_close(ftp[0].unwrap(), 9.50 + 9.0 / 91.0 * 0.60, "100-day FTP");
    assert_close(ftp[1].unwrap(), 8.75 + 1.0 / 61.0 * 0.75, "31-day FTP");
    assert!((ftp[0].unwrap() - 9.559).abs() < 0.001 && (ftp[1].unwrap() - 8.762).abs() < 0.001);
    assert_eq!(str_col(&out, "FTP_Method"), vec![Some("Linear 91d-182d".to_string()), Some("Linear 30d-91d".to_string())]);
    // Liability spread uses the interpolated rate.
    assert_close(f64_col(&out, "FTP_Spread_Pct")[0].unwrap(), 9.50 + 9.0 / 91.0 * 0.60 - 9.0, "spread");

    let out = run(df.clone(), &default_bd_ftp_curve(), FtpInterpolation::NextPointUp);
    assert_eq!(f64_col(&out, "Benchmark_FTP_Rate"), vec![Some(10.10), Some(9.50)]);
    assert_eq!(str_col(&out, "FTP_Method"), vec![Some("Next point up 182d".to_string()), Some("Next point up 91d".to_string())]);

    // Outside the curve: flat. 10 days on a curve starting at 30d -> the 30-day rate;
    // 20 years -> the last point (both methods).
    let edge = df!(
        "Opndat" => [o, o, o],
        "Matdat" => [o + 10, o + 7300, o + 30],
        "Balance" => [1.0, 1.0, 1.0],
        "Rate" => [9.0, 9.0, 9.0]
    )
    .unwrap()
    .lazy()
    .with_columns([col("Opndat").cast(DataType::Date), col("Matdat").cast(DataType::Date)]);
    let from_30: Vec<FtpPoint> = default_bd_ftp_curve().into_iter().filter(|p| p.tenor_days >= 30).collect();
    for m in [FtpInterpolation::Linear, FtpInterpolation::NextPointUp] {
        let out = run(edge.clone(), &from_30, m);
        assert_eq!(f64_col(&out, "Benchmark_FTP_Rate"), vec![Some(8.75), Some(11.75), Some(8.75)], "{m:?}");
    }
    let out = run(edge.clone(), &from_30, FtpInterpolation::Linear);
    let methods = str_col(&out, "FTP_Method");
    assert!(methods[0].as_deref().unwrap().starts_with("First point 30d"), "{methods:?}");
    assert!(methods[1].as_deref().unwrap().starts_with("Last point 1825d"), "{methods:?}");
    // On the full default curve (1d, 7d, 30d …) a 10-day deal lies between 7d and 30d.
    let out = run(edge.clone(), &default_bd_ftp_curve(), FtpInterpolation::Linear);
    assert_close(f64_col(&out, "Benchmark_FTP_Rate")[0].unwrap(), 8.25 + 3.0 / 23.0 * 0.50, "10-day on default curve");
    // Unsorted input curve gives the same result.
    let mut shuffled = default_bd_ftp_curve();
    shuffled.reverse();
    let out2 = run(edge.clone(), &shuffled, FtpInterpolation::Linear);
    assert_eq!(f64_col(&out, "Benchmark_FTP_Rate"), f64_col(&out2, "Benchmark_FTP_Rate"));

    // Bad curves fail loudly.
    let err = apply_ftp_curve_map(edge.clone(), "Opndat", "Matdat", "Balance", "Rate", &[], true, FtpInterpolation::Linear).err().expect("empty");
    assert!(format!("{err:#}").contains("no points"));
    let dup = vec![FtpPoint { tenor_days: 30, ftp_rate: 8.0 }, FtpPoint { tenor_days: 30, ftp_rate: 9.0 }];
    let err = apply_ftp_curve_map(edge, "Opndat", "Matdat", "Balance", "Rate", &dup, true, FtpInterpolation::Linear).err().expect("dup");
    assert!(format!("{err:#}").contains("two points at 30 days"), "{err:#}");

    // A v1.1.1 macro's FTP step (no interpolation field) loads as Linear.
    let json = r#"{"FtpCurveMap":{"opndat_col":"Opndat","matdat_col":"Matdat","bal_col":"Balance","rate_col":"Rate","curve":[{"tenor_days":30,"ftp_rate":8.75}],"is_liability":true}}"#;
    let step: TransformStep = serde_json::from_str(json).unwrap();
    assert!(matches!(step, TransformStep::FtpCurveMap { interpolation: FtpInterpolation::Linear, .. }));
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
    // A2 (closed) and A3 (new): no rate change (null, not ±rate × 100).
    let bps = f64_col(&out, "Rate_Delta_Bps");
    assert_eq!((bps[1], bps[2]), (None, None));
    assert_eq!(f64_col(&out, "Prev_Row_Count"), vec![Some(2.0), Some(1.0), Some(0.0)]);
    assert_eq!(f64_col(&out, "Curr_Row_Count"), vec![Some(1.0), Some(0.0), Some(1.0)]);
    let rec = build_period_reconcile_lazy(reconcile_frames().0.lazy(), reconcile_frames().1.lazy(), "Actnum", "Balance", Some("Rate"), None).unwrap();
    assert_eq!(reconcile_multi_row_keys(rec).unwrap(), (1, 0));

    // Null-safe weighting: a row without a rate (or balance) is left out of both sums;
    // a key whose known rows have zero balance has no rate.
    let prev = df!(
        "Actnum" => ["B1", "B1", "B1", "B2"],
        "Balance" => [Some(300.0), Some(100.0), None, Some(0.0)],
        "Rate" => [None, Some(8.0), Some(99.0), Some(7.0)]
    )
    .unwrap();
    let curr = df!("Actnum" => ["B1", "B2"], "Balance" => [100.0, 50.0], "Rate" => [8.0, 7.0]).unwrap();
    let out = build_period_reconcile_lazy(prev.lazy(), curr.lazy(), "Actnum", "Balance", Some("Rate"), None)
        .unwrap()
        .sort(["Actnum"], Default::default())
        .collect()
        .unwrap();
    assert_eq!(f64_col(&out, "Prev_Rate"), vec![Some(8.0), None]);
    assert_eq!(f64_col(&out, "Rate_Delta_Bps"), vec![Some(0.0), None]);

    // CL per key: the worst grade by the CL map (STD < SMA < SS < DF < BL); unmapped codes ->
    // the code of the largest-balance row.
    let cl_map: Vec<(String, String)> = [("STD", "STD"), ("SMA", "SMA"), ("SS", "SS"), ("DF", "DF"), ("BL", "BL"), ("UC", "STD")]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    let prev = df!(
        "Actnum" => ["C1", "C1", "C1", "C2", "C2", "C3"],
        "Balance" => [900.0, 50.0, 50.0, 10.0, 500.0, 1.0],
        "CL" => ["UC", "ss", "SMA", "X9", "Y7", "BL"]
    )
    .unwrap();
    let curr = df!("Actnum" => ["C1", "C2", "C3"], "Balance" => [1.0, 1.0, 1.0], "CL" => ["DF", "Y7", "BL"]).unwrap();
    let out = build_period_reconcile_lazy_with(prev.lazy(), curr.lazy(), "Actnum", "Balance", None, Some("CL"), &cl_map)
        .unwrap()
        .sort(["Actnum"], Default::default())
        .collect()
        .unwrap();
    assert_eq!(
        str_col(&out, "Prev_CL"),
        vec![Some("ss".to_string()), Some("Y7".to_string()), Some("BL".to_string())]
    );
    assert_eq!(str_col(&out, "CL_Migration_Matrix")[0].as_deref(), Some("ss -> DF"));
}

#[test]
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
            ..Default::default()
        }],
        ..Default::default()
    };
    let df = build_lazy_pipeline(&p, false, &cfg, &tmpdir()).unwrap().collect().unwrap();
    let names = df.column("Name").unwrap();
    assert_eq!(names.len(), n);
    assert_eq!(names.n_unique().unwrap(), n, "masked names must stay distinct");
    let first = names.str().unwrap().get(0).unwrap().to_string();
    assert!(first.starts_with("CLIENT_") && first.len() == 7 + 16, "{first}");

    // Same input + same key = same token; different keys = different tokens.
    let k1 = [7u8; 32];
    let mut k2 = [7u8; 32];
    k2[31] = 8;
    let m = |v: &str, mode, k: &[u8; 32]| mask_pii_value(v, mode, k);
    assert_eq!(m("RAHIM UDDIN", PiiMaskMode::ClientNameHash, &k1), m(" RAHIM UDDIN ", PiiMaskMode::ClientNameHash, &k1));
    assert_ne!(m("RAHIM UDDIN", PiiMaskMode::ClientNameHash, &k1), m("RAHIM UDDIN", PiiMaskMode::ClientNameHash, &k2));
    assert_eq!(m("   ", PiiMaskMode::AccountNumber, &k1), "");
    // Account tokens keep no run of 3+ digits of the original.
    for i in 0..2000u64 {
        let acct = format!("{:013}", 1_234_567_000_000u64 + i * 7_919);
        for k in [&k1, &k2] {
            let t = m(&acct, PiiMaskMode::AccountNumber, k);
            assert!(t.starts_with("ACCT_") && t.len() == 21, "{t}");
            assert!(t[5..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()), "{t}");
            let digits: Vec<char> = acct.chars().collect();
            for w in digits.windows(3) {
                let run: String = w.iter().collect();
                assert!(!t.contains(&run), "{t} contains {run} of {acct}");
            }
        }
    }
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
        ..Default::default()
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
    // Amounts go through the same parsing rules as the app (P1), with the column's sign setting.
    assert!(py.contains("def _mp_amount(v, col, sign, acc, as_int=False):"), "{py}");
    assert!(
        py.contains("pl.col(\"Balance\").map_elements(lambda v: _mp_amount(v, \"Balance\", \"Unset\", True), return_dtype=pl.Float64)"),
        "Float64 columns must use the amount parser:\n{py}"
    );
    // Dates go through the same parser as the app (P3), not a fixed strptime format.
    assert!(py.contains("def _mp_date(v, mode):"), "{py}");
    assert!(py.contains("pl.col(\"Matdat\").map_elements(lambda v: _mp_date(v, \"DateDMY\"), return_dtype=pl.Date)"), "{py}");
    assert!(py.contains("pl.col(\"Opndat\").map_elements(lambda v: _mp_date(v, \"DateMDY\"), return_dtype=pl.Date)"), "{py}");
    assert!(py.contains("pl.col(\"Valdat\").map_elements(lambda v: _mp_date(v, \"DateISO\"), return_dtype=pl.Date)"), "{py}");
    assert!(!py.contains("str.to_date("), "{py}");
    // Casts are not strict (bad values become null, as in the app) and the null counts are printed
    assert!(!py.contains("strict=True"), "{py}");
    assert!(py.contains("_cast_cols = [\"Balance\", \"Matdat\", \"Opndat\", \"Valdat\"]"), "{py}");
    assert!(py.contains("could not be converted and became null"), "{py}");
    let missing = micro_parquet::macros::python_untranslated_steps(&steps);
    assert_eq!(missing, vec!["Step 1 '+BB CL Provision & BFP'".to_string(), "Step 2 '+ΔNII Shock (+100 bps)'".to_string()]);
}

#[test]
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
    assert_eq!(str_col(&df, "Source_File"), vec![Some("sep.csv".to_string()), Some("oct.csv".to_string())]);
    let names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    assert_eq!(names, vec!["Actnum", "Balance", "Branch", "Source_File"]);
}

#[test]
fn t16b_stack_type_mismatch_and_shared_schema() {
    let a = write_csv("sep_t.csv", "Actnum,Balance\n001,100\n");
    let b = write_csv("oct_t.csv", "Actnum,Balance\n002,abc\n");
    let files = vec![a.clone(), b.clone()];
    let (specs, notes) = stack_file_configs(&files, None).unwrap();
    assert!(notes.is_empty());
    assert_eq!(dtype_of(&specs[1].2, "Balance"), ColDataType::Utf8);
    let src = TabSource::VerticalStack { files: specs, include_source_col: true };
    let err = compile_tab_lazy(&src, &[], &tmpdir()).err().expect("types differ: error");
    let msg = format!("{err:#}");
    assert!(msg.contains("Balance") && msg.contains("sep_t.csv") && msg.contains("oct_t.csv"), "{msg}");
    assert!(msg.contains("Float64") && msg.contains("String"), "{msg}");

    // The first file's schema applied to both: loads, "abc" is a counted parse failure.
    let shared = detect_file_schema(&a).unwrap();
    let (specs, _) = stack_file_configs(&files, Some(&shared)).unwrap();
    let src = TabSource::VerticalStack { files: specs, include_source_col: true };
    let df = compile_tab_lazy(&src, &[], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(df.height(), 2);
    assert_eq!(f64_col(&df, "Balance"), vec![Some(100.0), None]);
    let fails = source_parse_failures(&src, &tmpdir()).unwrap();
    assert!(fails.iter().any(|f| f.column == "Balance" && f.count == 1), "{fails:?}");

    // A saved schema with a column a file lacks: a note, and the column is null there.
    let mut saved = shared.clone();
    let mut extra = saved.columns[0].clone();
    extra.name = "Branch".into();
    saved.columns.push(extra);
    let c = write_csv("nov_t.csv", "Actnum,Balance,Branch\n003,300,CTG\n");
    let (specs, notes) = stack_file_configs(&[a.clone(), c.clone()], Some(&saved)).unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("sep_t.csv") && notes[0].contains("Branch"), "{notes:?}");
    let src = TabSource::VerticalStack { files: specs, include_source_col: false };
    let df = compile_tab_lazy(&src, &[], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(str_col(&df, "Branch"), vec![None, Some("CTG".to_string())]);

    // A Parquet file with Float64 Balance stacks with a CSV of the same column.
    let p = write_parquet("dec_t.parquet", &df!("Actnum" => ["004"], "Balance" => [400.0]).unwrap());
    let (specs, _) = stack_file_configs(&[a.clone(), p.clone()], None).unwrap();
    let src = TabSource::VerticalStack { files: specs, include_source_col: false };
    let df = compile_tab_lazy(&src, &[], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&df, "Balance"), vec![Some(100.0), Some(400.0)]);

    // An unreadable file is an error, not an empty schema.
    let err = stack_file_configs(&[tmpdir().join("no_such_file.csv")], None).err().expect("missing file");
    assert!(format!("{err:#}").contains("no_such_file.csv"));
}

#[test]
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
        keep_first_duplicate_right: false,
    };
    // Target: a right side with a repeated key fails loudly, naming the key, instead of
    // doubling the left row (BDT 1,000,000 would become 2,000,000).
    let err = compile_tab_lazy(&src, &[], &tmpdir())
        .and_then(|lf| compute_control_totals(lf, Some("Balance")))
        .err()
        .expect("duplicate right-side keys must be an error");
    let msg = format!("{err:#}");
    assert!(msg.contains("A1") && msg.to_lowercase().contains("duplicate"), "{msg}");
    assert!(msg.contains("Actnum") && msg.contains("Keep first row per key"), "{msg}");

    // Semi/anti joins never repeat a left row: no check.
    let mut semi = src.clone();
    if let TabSource::JoinedTabs { join_kind, .. } = &mut semi {
        *join_kind = SqlJoinKind::Semi;
    }
    let (rows, _) = compile_tab_lazy(&semi, &[], &tmpdir())
        .and_then(|lf| compute_control_totals(lf, Some("Balance")))
        .unwrap();
    assert_eq!(rows, 1);

    // "Keep first row per key": 1 row, sum unchanged, the first right row is used, warning count 1.
    let mut keep = src.clone();
    if let TabSource::JoinedTabs { keep_first_duplicate_right, .. } = &mut keep {
        *keep_first_duplicate_right = true;
    }
    let df = compile_tab_lazy(&keep, &[], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(df.height(), 1);
    assert_eq!(f64_col(&df, "Balance"), vec![Some(1_000_000.0)]);
    assert_eq!(str_col(&df, "Sector"), vec![Some("RMG".to_string())]);
    let js = join_status(&keep, &[], Some("Balance"), &tmpdir()).unwrap().expect("join status");
    assert_eq!((js.rows, js.left_rows), (1, 1));
    assert_eq!(js.sum, Some(1_000_000.0));
    assert_eq!(js.left_sum, Some(1_000_000.0));
    assert_eq!((js.duplicate_keys, js.dropped_rows), (1, 1));
    let s = js.summary();
    assert!(s.contains("1 duplicated right key") && s.contains("1 right row(s) dropped"), "{s}");

    // Null keys on the right are not duplicates (they never match) and survive "keep first".
    let right_nulls = df!("Actnum" => [None::<&str>, None, Some("A1")], "Sector" => ["x", "y", "RMG"]).unwrap();
    let prn = write_parquet("r_nulls.parquet", &right_nulls);
    let mut nulls = src.clone();
    if let TabSource::JoinedTabs { right_source, join_kind, .. } = &mut nulls {
        *right_source = Box::new(TabSource::SingleFile { path: prn, is_parquet: true, config: IngestionConfig::default() });
        *join_kind = SqlJoinKind::Full;
    }
    let df = compile_tab_lazy(&nulls, &[], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(df.height(), 3);
    if let TabSource::JoinedTabs { keep_first_duplicate_right, .. } = &mut nulls {
        *keep_first_duplicate_right = true;
    }
    let df = compile_tab_lazy(&nulls, &[], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(df.height(), 3);

    // Not a join: no join status.
    let single = TabSource::SingleFile { path: write_parquet("l2.parquet", &left), is_parquet: true, config: IngestionConfig::default() };
    assert!(join_status(&single, &[], Some("Balance"), &tmpdir()).unwrap().is_none());
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
    // Active statuses are a setting (default ACT, ACTIVE, A, OPEN), trimmed and case-insensitive.
    let past = days(2025, 1, 1);
    let open = days(2024, 1, 1);
    let future = days(2027, 1, 1);
    let df = df!(
        "Opndat" => [open, open, open, open, open, open],
        "Matdat" => [past, past, past, future, past, future],
        "Clsdat" => [None::<i32>, None, None, None, None, None],
        "Acstat" => ["ACT", "Active", " open ", "A", "CLOSED", "ACT"],
        "Balance" => [500.0, 500.0, 500.0, 500.0, 500.0, 500.0],
        "Rate" => [9.0, 30.0, 9.0, 9.0, 9.0, 9.0]
    )
    .unwrap()
    .lazy()
    .with_columns([
        col("Opndat").cast(DataType::Date),
        col("Matdat").cast(DataType::Date),
        col("Clsdat").cast(DataType::Date),
    ]);
    let run = |statuses: &[String], lo: f64, hi: f64| {
        apply_banking_hygiene_audit(df.clone(), "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false, statuses, lo, hi)
            .unwrap()
            .collect()
            .unwrap()
    };
    let out = run(&default_active_statuses(), 0.0, 25.0);
    let flags = str_col(&out, "Audit_Exception_Flag");
    let all = str_col(&out, "Audit_All_Flags");
    assert_eq!(flags[0].as_deref(), Some("1. GHOST_MATURED_ACTIVE"));
    // "Active" with a past maturity and positive balance is flagged; its 30% rate is also listed.
    assert_eq!(flags[1].as_deref(), Some("1. GHOST_MATURED_ACTIVE"));
    assert_eq!(all[1].as_deref(), Some("1. GHOST_MATURED_ACTIVE; 3. INTEREST_RATE_OUTLIER"));
    assert_eq!(flags[2].as_deref(), Some("1. GHOST_MATURED_ACTIVE"), "' open ' is active");
    assert_eq!(flags[3].as_deref(), Some("OK"), "not matured");
    assert_eq!(flags[4].as_deref(), Some("OK"), "CLOSED is not active");
    assert_eq!(all[0].as_deref(), Some("1. GHOST_MATURED_ACTIVE"));
    assert_eq!(all[5].as_deref(), Some("OK"));

    // Settings: only ACT is active; outliers above 8%.
    let out = run(&["act".to_string()], 0.0, 8.0);
    let flags = str_col(&out, "Audit_Exception_Flag");
    assert_eq!(flags[1].as_deref(), Some("3. INTEREST_RATE_OUTLIER"));
    assert_eq!(flags[5].as_deref(), Some("3. INTEREST_RATE_OUTLIER"));
    // Bad settings fail loudly.
    let bad = |s: &[String], lo: f64, hi: f64| {
        apply_banking_hygiene_audit(df.clone(), "Opndat", "Matdat", "Clsdat", "Acstat", "Balance", "Rate", "2026-10-31", false, s, lo, hi).is_err()
    };
    assert!(bad(&[" ".to_string()], 0.0, 25.0));
    assert!(bad(&default_active_statuses(), 30.0, 25.0));

    // A v1.1.1 macro's audit step loads with the defaults.
    let json = r#"{"BankingHygieneAudit":{"opndat_col":"O","matdat_col":"M","clsdat_col":"C","acstat_col":"S","bal_col":"B","rate_col":"R","actnum_col":"A","cutoff_date":"2026-10-31","only_exceptions":false}}"#;
    match serde_json::from_str::<TransformStep>(json).unwrap() {
        TransformStep::BankingHygieneAudit { active_statuses, rate_min_pct, rate_max_pct, .. } => {
            assert_eq!(active_statuses, default_active_statuses());
            assert_eq!((rate_min_pct, rate_max_pct), (0.0, 25.0));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn t19b_blank_balances_reported() {
    let df = df!(
        "Actnum" => ["L1", "L2", "L3", "L4"],
        "Balance" => [Some(1_000_000.0), None, None, Some(500.0)],
        "CL" => ["STD", "SS", "BL", "STD"]
    )
    .unwrap();
    let src = TabSource::SingleFile { path: write_parquet("t19b.parquet", &df), is_parquet: true, config: IngestionConfig::default() };
    let prov = TransformStep::LoanClassificationProvision {
        bal_col: "Balance".into(),
        cl_status_col: "CL".into(),
        segment_col: None,
        suspense_col: None,
        security_col: None,
        cutoff_date: "2026-10-31".into(),
        collateral_type_col: None,
    };
    let steps = vec![prov];
    // The treatment does not change (the step still runs) ...
    let out = compile_tab_lazy(&src, &steps, &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(out.height(), 4);
    // ... and the blank balances are reported.
    let notes = blank_balance_notes(&src, &steps, &tmpdir()).unwrap();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].ends_with("Balance blank on 2 rows; treated as 0"), "{notes:?}");
    // No banking step, or no blanks: no note.
    assert!(blank_balance_notes(&src, &[], &tmpdir()).unwrap().is_empty());
    assert_eq!(count_blank_values(df.lazy(), "Actnum").unwrap(), 4, "text is not a balance");
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
fn t20b_phase1_schema_json_still_loads() {
    // A .schema.json saved by a Phase 1 build (no Phase 2 fields)
    let json = r#"{
        "columns": [
            {"name": "Actnum", "dtype": "Utf8", "pii_mask": "AccountNumber", "sample_values": ["0012345"]},
            {"name": "Balance", "dtype": "Float64", "pii_mask": "None", "sample_values": ["1,234.50"]},
            {"name": "Matdat", "dtype": "DateDMY", "pii_mask": "None", "sample_values": ["05/11/2026"]}
        ],
        "sanitize_linebreaks": true,
        "parse_accounting_negatives": true,
        "heal_broken_csv_lines": false,
        "pii_salt": "BANK_ASIA_ALM_2026"
    }"#;
    let cfg: IngestionConfig = serde_json::from_str(json).expect("Phase 1 schema JSON must load");
    assert_eq!(cfg.columns.len(), 3);
    let bal = &cfg.columns[1];
    assert_eq!(bal.dtype, ColDataType::Float64);
    assert_eq!(bal.sign_convention, SignConvention::Unset);
    assert!(!bal.has_dr_cr);
    assert_eq!(cfg.separator, ',');
    assert_eq!(cfg.skip_rows, 0);
    assert_eq!(cfg.encoding, CsvTextEncoding::Utf8);
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
    let cur_tag = format!("\"defaults_version\": {RULE_FILES_VERSION}");
    assert!(std::fs::read_to_string(dir.join("segment_map.json")).unwrap().contains(&cur_tag));

    // Edited files at the current version load without a warning
    let mut text = std::fs::read_to_string(dir.join("cl_code_map.json")).unwrap();
    text = text.replacen('{', &format!("{{\n  {cur_tag},"), 1);
    std::fs::write(dir.join("cl_code_map.json"), text).unwrap();
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
}

/// Writes the three rule files as the version-2 build wrote them (versioned layout).
fn write_v2_rule_files(dir: &std::path::Path, rules: &[ProvisionRule]) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let flat = |pairs: Vec<(&str, &str)>| {
        let mut body = vec!["  \"defaults_version\": 2".to_string()];
        body.extend(pairs.iter().map(|(k, v)| format!("  {:?}: {:?}", k, v)));
        format!("{{\n{}\n}}\n", body.join(",\n"))
    };
    std::fs::write(dir.join("cl_code_map.json"), flat(default_cl_code_map(2))).unwrap();
    std::fs::write(dir.join("segment_map.json"), flat(default_segment_map(2))).unwrap();
    let file = serde_json::json!({ "defaults_version": 2, "rules": rules });
    std::fs::write(dir.join("provision_rules.json"), serde_json::to_string_pretty(&file).unwrap()).unwrap();
}

#[test]
fn t07d_rule_files_v2_to_v3() {
    assert_eq!(RULE_FILES_VERSION, 3);
    let v3 = ProvisionConfig::default_rules();
    let concession: Vec<&ProvisionRule> = v3.iter().filter(|r| r.effective_from.starts_with("2025-1")).collect();
    assert_eq!(concession.len(), 8, "Oct and Dec 2025 rows");
    assert!(concession.iter().all(|r| !r.verify), "v3 rows are checked");
    let dec: Vec<_> = concession.iter().filter(|r| r.effective_from == "2025-12-21").collect();
    assert_eq!(dec.len(), 4);
    assert!(dec.iter().all(|r| r.source == "BRPD Circular Letter No. 29, 21 Dec 2025" && r.rate_pct == 0.5));
    assert!(concession.iter().filter(|r| r.effective_from == "2025-10-13").all(|r| r.source == "BRPD Circular Letter No. 22, 13 Oct 2025" && r.rate_pct == 1.0));
    // Only verify and the Dec source differ from v2; rates and dates are unchanged
    let v2 = ProvisionConfig::default_rules_for(2);
    assert_eq!(v2.len(), v3.len());
    for (a, b) in v2.iter().zip(&v3) {
        assert_eq!((&a.grade, &a.category, a.rate_pct, &a.effective_from, &a.effective_to), (&b.grade, &b.category, b.rate_pct, &b.effective_from, &b.effective_to));
    }
    assert!(v2.iter().filter(|r| r.effective_from.starts_with("2025-1")).all(|r| r.verify));

    // Untouched v2 files are upgraded to v3 silently
    let dir = tmpdir().join("t07d_untouched");
    write_v2_rule_files(&dir, &v2);
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
    assert_eq!(cfg.rules, v3);
    for f in ["cl_code_map.json", "segment_map.json", "provision_rules.json"] {
        let text = std::fs::read_to_string(dir.join(f)).unwrap();
        assert!(text.contains("\"defaults_version\": 3"), "{f}: {text}");
    }
    let text = std::fs::read_to_string(dir.join("provision_rules.json")).unwrap();
    assert!(text.contains("Circular Letter No. 29") && !text.contains("\"verify\": true"), "{text}");

    // An edited v2 rules file is kept with a warning; its rows stay flagged
    let dir = tmpdir().join("t07d_edited");
    let mut edited = v2.clone();
    edited[0].source = "BRPD Circular 15, 27 Nov 2024 (bank copy)".into();
    write_v2_rule_files(&dir, &edited);
    let before = std::fs::read_to_string(dir.join("provision_rules.json")).unwrap();
    let cfg = ProvisionConfig::load_or_create(&dir).unwrap();
    assert_eq!(cfg.warnings.len(), 1, "{:?}", cfg.warnings);
    assert!(cfg.warnings[0].contains("provision_rules.json"), "{}", cfg.warnings[0]);
    assert_eq!(std::fs::read_to_string(dir.join("provision_rules.json")).unwrap(), before);
    assert_eq!(cfg.rules, edited);
    assert!(std::fs::read_to_string(dir.join("segment_map.json")).unwrap().contains("\"defaults_version\": 3"));
}

#[test]
fn t25_split_export_keeps_every_row() {
    use micro_parquet::exporters::split_and_export_by_column;
    let df = df!(
        "Branch" => [Some("DHK"), Some("dhk"), None, Some("A/B"), Some("A_B"), Some("  "), Some("DHK")],
        "Balance" => [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0]
    )
    .unwrap();
    let out = tmpdir().join("split_t25");
    let _ = std::fs::remove_dir_all(&out);
    let n = split_and_export_by_column(df.clone().lazy(), "Branch", &out, SplitExportFormat::Csv, ParquetDialect::ArchiveZstd, false, Some("Balance"))
        .unwrap();
    assert_eq!(n, 5);
    let mut files: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    assert_eq!(files, vec!["A_B.csv", "A_B_2.csv", "BLANK_KEY.csv", "DHK.csv", "dhk_2.csv", "split_manifest.csv"]);
    let man = CsvReadOptions::default()
        .with_infer_schema_length(Some(0))
        .try_into_reader_with_file_path(Some(out.join("split_manifest.csv")))
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(man.height(), 5);
    let rows: usize = str_col(&man, "rows").iter().map(|r| r.as_deref().unwrap().parse::<usize>().unwrap()).sum();
    assert_eq!(rows, df.height());
    // Null and whitespace-only keys share BLANK_KEY (rows 4 + 32).
    let blank_row = str_col(&man, "file").iter().position(|f| f.as_deref() == Some("BLANK_KEY.csv")).unwrap();
    assert_eq!(str_col(&man, "rows")[blank_row].as_deref(), Some("2"));
    assert_eq!(str_col(&man, "control_sum")[blank_row].as_deref(), Some("36.0"));
    assert_eq!(str_col(&man, "key")[blank_row], None);
    let dhk = str_col(&man, "key").iter().position(|k| k.as_deref() == Some("DHK")).unwrap();
    assert_eq!(str_col(&man, "rows")[dhk].as_deref(), Some("2"));
    let body = std::fs::read_to_string(out.join("dhk_2.csv")).unwrap();
    assert!(body.contains("dhk") && !body.contains("DHK"), "{body}");

    // Parquet: same partition.
    let outp = tmpdir().join("split_t25p");
    let _ = std::fs::remove_dir_all(&outp);
    let n = split_and_export_by_column(df.lazy(), "Branch", &outp, SplitExportFormat::Parquet, ParquetDialect::ArchiveZstd, false, None).unwrap();
    assert_eq!(n, 5);
    let total: usize = ["A_B", "A_B_2", "BLANK_KEY", "DHK", "dhk_2"]
        .iter()
        .map(|s| {
            LazyFrame::scan_parquet(outp.join(format!("{s}.parquet")), Default::default()).unwrap().collect().unwrap().height()
        })
        .sum();
    assert_eq!(total, 7);
}

fn unzip_entry(zip: &std::path::Path, entry: &str) -> String {
    let out = std::process::Command::new("unzip").arg("-p").arg(zip).arg(entry).output().expect("unzip must be installed");
    assert!(out.status.success(), "unzip {entry}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn t26_excel_dates_sheets_and_errors() {
    use micro_parquet::exporters::*;
    let d = days(2026, 10, 31);
    let df = df!("Actnum" => ["A1", "A2", "A3", "A4", "A5"], "Balance" => [1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
    let mut df = df;
    df.with_column(Series::new("Matdat".into(), vec![d, d, d, d, d]).cast(&DataType::Date).unwrap()).unwrap();
    // 2026-10-31 12:00:00 in microseconds
    let us = (d as i64) * 86_400_000_000 + 43_200_000_000;
    df.with_column(
        Series::new("Stamp".into(), vec![us; 5]).cast(&DataType::Datetime(TimeUnit::Microseconds, None)).unwrap(),
    )
    .unwrap();

    let p = tmpdir().join("t26.xlsx");
    let rep = export_tabs_to_xlsx(vec![("Loans".into(), df.clone())], Vec::new(), &p).unwrap();
    assert_eq!(rep.sheets, vec![("Loans".to_string(), 1)]);
    let sheet = unzip_entry(&p, "xl/worksheets/sheet1.xml");
    let styles = unzip_entry(&p, "xl/styles.xml");
    let serial = d + 25_569;
    // Matdat is column C: a number cell (no t="s"/"str") holding the date serial, with a style.
    let c2 = sheet.split("<c r=\"C2\"").nth(1).expect("C2 cell").split("</c>").next().unwrap().to_string();
    assert!(c2.contains(&format!("<v>{serial}</v>")) && c2.contains(" s=\"") && !c2.contains("t=\""), "{c2}");
    assert!(styles.contains("formatCode=\"dd-mmm-yyyy\""), "{styles}");
    assert!(styles.contains("formatCode=\"dd-mmm-yyyy hh:mm:ss\""), "{styles}");
    let d2 = sheet.split("<c r=\"D2\"").nth(1).expect("D2 cell").split("</c>").next().unwrap().to_string();
    assert!(d2.contains(&format!("<v>{serial}.5</v>")), "{d2}");

    // More rows than a sheet holds: continues on "<sheet> (2)", "(3)" instead of truncating.
    let p2 = tmpdir().join("t26_spill.xlsx");
    let failing = vec![("Broken tab".to_string(), "Column 'X' not found".to_string())];
    let rep = write_xlsx_workbook(vec![("Loans".into(), df.clone())], failing, &p2, 2).unwrap();
    assert_eq!(rep.sheets, vec![("Loans".to_string(), 3)]);
    let wb = unzip_entry(&p2, "xl/workbook.xml");
    for name in ["1_Loans\"", "1_Loans (2)\"", "1_Loans (3)\"", "Export_Errors\""] {
        assert!(wb.contains(&format!("name=\"{name}")), "{name} in {wb}");
    }
    let s3 = unzip_entry(&p2, "xl/worksheets/sheet3.xml");
    assert!(s3.contains("<c r=\"B2\"") && !s3.contains("<c r=\"B3\""), "last sheet holds row 5 only: {s3}");
    let strings = unzip_entry(&p2, "xl/sharedStrings.xml");
    assert!(strings.contains("Broken tab") && strings.contains("Column 'X' not found"), "{strings}");
    let summary = rep.summary();
    assert!(summary.contains("Export_Errors") && summary.contains("Broken tab") && summary.contains("3 sheets"), "{summary}");

    // Nothing to export at all is an error, not an empty workbook.
    assert!(export_tabs_to_xlsx(vec![], vec![], &tmpdir().join("t26_empty.xlsx")).is_err());
}

#[test]
fn t26b_pdf_omitted_columns_and_rows() {
    use micro_parquet::exporters::*;
    assert_eq!(pdf_column_layout(4).0, 4);
    let (shown, w) = pdf_column_layout(20);
    assert_eq!(shown, 15);
    assert!(w >= 18.0 && shown as f32 * w <= 273.0 + 0.01);
    let cols: Vec<Series> = (0..20).map(|i| Series::new(format!("C{i:02}").into(), vec![1.0f64, 2.0])).collect();
    let df = DataFrame::new(cols).unwrap();
    let p = tmpdir().join("t26b.pdf");
    let rep = export_tabs_to_pdf(vec![("Wide".into(), df, Default::default())], &p, NumberDisplayFormat::Raw, 2).unwrap();
    assert_eq!(rep.omitted_columns, vec![("Wide".to_string(), (15..20).map(|i| format!("C{i:02}")).collect::<Vec<_>>())]);
    assert!(rep.summary().contains("C15, C16, C17, C18, C19 omitted"), "{}", rep.summary());
    assert!(rep.truncated_tabs.is_empty());

    let long = df!("Balance" => vec![1.0f64; PDF_MAX_ROWS_PER_TAB + 1]).unwrap();
    let rep = export_tabs_to_pdf(vec![("Long".into(), long, Default::default())], &tmpdir().join("t26c.pdf"), NumberDisplayFormat::Raw, 2).unwrap();
    assert_eq!(rep.truncated_tabs, vec!["Long".to_string()]);
    assert!(rep.omitted_columns.is_empty());
}

#[test]
fn t27_batch_log_and_folder_checks() {
    use micro_parquet::batch::*;
    let base = tmpdir().join("t27");
    let _ = std::fs::remove_dir_all(&base);
    let inp = base.join("in");
    std::fs::create_dir_all(inp.join("branch")).unwrap();
    std::fs::write(inp.join("good.csv"), "Actnum,Balance\n001,100.5\n002,200\n").unwrap();
    std::fs::write(inp.join("branch").join("malformed.csv"), "").unwrap();
    let out = base.join("out");
    let rep = run_folder_mirror_batch(&inp, &out, BatchDirection::CsvToParquet, BatchSchemaStrategy::SmartSafeAutoDetect, None, ParquetDialect::ArchiveZstd, false, &base)
        .unwrap();
    assert_eq!(rep.files_converted, 1);
    assert_eq!(rep.errors.len(), 1);
    assert!(rep.errors[0].contains("malformed.csv"), "{:?}", rep.errors);
    let log = CsvReadOptions::default()
        .with_infer_schema_length(Some(0))
        .try_into_reader_with_file_path(Some(out.join("batch_log.csv")))
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(log.height(), 2);
    let status = str_col(&log, "status");
    let file = str_col(&log, "file");
    let ok = status.iter().position(|s| s.as_deref() == Some("OK")).expect("one OK row");
    let bad = status.iter().position(|s| s.as_deref() == Some("ERROR")).expect("one ERROR row");
    assert_eq!(file[ok].as_deref(), Some("good.csv"));
    assert_eq!(str_col(&log, "rows")[ok].as_deref(), Some("2"));
    assert_eq!(str_col(&log, "control_sum")[ok].as_deref(), Some("300.5"));
    assert_eq!(str_col(&log, "parse_failures")[ok].as_deref(), Some("0"));
    assert!(file[bad].as_deref().unwrap().contains("malformed.csv"));
    assert!(str_col(&log, "error")[bad].as_deref().unwrap().contains("no header row"));
    assert!(out.join("good.parquet").exists());

    // Output folder inside the input folder (or equal to it) is refused before anything is written.
    for o in [inp.join("converted"), inp.clone(), inp.join("branch").join("..").join("x")] {
        let err = run_folder_mirror_batch(&inp, &o, BatchDirection::CsvToParquet, BatchSchemaStrategy::SmartSafeAutoDetect, None, ParquetDialect::ArchiveZstd, false, &base)
            .err()
            .expect("output inside input");
        assert!(format!("{err:#}").contains("inside the input folder"), "{err:#}");
    }
    assert!(!inp.join("converted").exists());
    // A sibling folder whose name starts with the input's name is not inside it.
    assert!(check_batch_folders(&inp, &base.join("in_out")).is_ok());

    // Template mode without a template is an error, not a silent auto-detect.
    let err = run_folder_mirror_batch(&inp, &base.join("out2"), BatchDirection::CsvToParquet, BatchSchemaStrategy::MasterSchemaTemplate, None, ParquetDialect::ArchiveZstd, false, &base)
        .err()
        .expect("no template");
    assert!(format!("{err:#}").contains("no template"), "{err:#}");

    // Parquet -> CSV: companion files are copied and logged; rows and sum come from the source.
    let pin = base.join("pin");
    std::fs::create_dir_all(&pin).unwrap();
    let mut df = df!("Actnum" => ["A1", "A2"], "Balance" => [1.0, 2.0]).unwrap();
    ParquetWriter::new(std::fs::File::create(pin.join("m.parquet")).unwrap()).finish(&mut df).unwrap();
    std::fs::write(pin.join("readme.txt"), "notes").unwrap();
    let rep = run_folder_mirror_batch(&pin, &base.join("pout"), BatchDirection::ParquetToCsv, BatchSchemaStrategy::SmartSafeAutoDetect, None, ParquetDialect::ArchiveZstd, false, &base)
        .unwrap();
    let mut log = rep.log.clone();
    log.sort_by(|a, b| a.file.cmp(&b.file));
    assert_eq!(log.len(), 2);
    assert_eq!((log[0].file.as_str(), log[0].status.as_str(), log[0].rows, log[0].control_sum), ("m.parquet", "OK", Some(2), Some(3.0)));
    assert_eq!((log[1].file.as_str(), log[1].status.as_str()), ("readme.txt", "COPIED"));
}

#[test]
fn t28_control_totals_state_and_math_scalar() {
    let df = df!("Branch" => ["DHK", "CTG", "DHK"], "Balance" => [1.0, 2.0, 4.0], "Name" => ["a", "b", "c"]).unwrap();
    let st = control_totals_state(Ok(df.clone().lazy()), Some("Balance"));
    assert_eq!(st, ControlTotals { rows: Some(3), sum: Some(7.0), note: None });
    let st = control_totals_state(Ok(df.clone().lazy()), None);
    assert_eq!((st.rows, st.sum, st.note), (Some(3), None, None));

    // After a pivot the control column is gone: sum "—" with the reason (rows still known).
    let pivot = df.clone().lazy().group_by([col("Branch")]).agg([col("Balance").sum().alias("Total")]);
    let st = control_totals_state(Ok(pivot), Some("Balance"));
    assert_eq!((st.rows, st.sum), (Some(2), None));
    assert!(st.note.as_deref().unwrap().contains("'Balance' no longer exists"), "{st:?}");

    // A text control column has no sum.
    let st = control_totals_state(Ok(df.clone().lazy()), Some("Name"));
    assert_eq!((st.rows, st.sum), (Some(3), None));
    assert!(st.note.as_deref().unwrap().contains("not a number"), "{st:?}");

    // A failing pipeline: "—" for rows and sum, with the error, never the previous values.
    let st = control_totals_state(Err(anyhow::anyhow!("Column 'X' not found")), Some("Balance"));
    assert_eq!((st.rows, st.sum), (None, None));
    assert!(st.note.as_deref().unwrap().contains("Column 'X' not found"));
    let bad = df.clone().lazy().select([col("Missing")]);
    let st = control_totals_state(Ok(bad), Some("Balance"));
    assert_eq!((st.rows, st.sum), (None, None));
    assert!(st.note.is_some());

    // Math column scalar.
    assert_eq!(parse_math_scalar("x", " 10,000,000 ").unwrap(), 10_000_000.0);
    assert_eq!(parse_math_scalar("x", "0.5").unwrap(), 0.5);
    for v in ["abc", "", "1.2.3", "NaN", "inf"] {
        let err = parse_math_scalar("Bal_Cr", v).err().expect("not a number");
        let msg = format!("{err:#}");
        assert!(msg.contains(&format!("'{v}'")) && msg.contains("Bal_Cr"), "{msg}");
    }
    let src = TabSource::SingleFile { path: write_parquet("t28.parquet", &df), is_parquet: true, config: IngestionConfig::default() };
    let step = |v: &str| TransformStep::MathFormula {
        new_col: "Bal_Cr".into(),
        left_col: "Balance".into(),
        op: MathOp::Div,
        right_is_col: false,
        right_val_or_col: v.into(),
    };
    let err = compile_tab_lazy(&src, &[step("ten million")], &tmpdir()).err().expect("bad scalar");
    assert!(format!("{err:#}").contains("'ten million' is not a number"), "{err:#}");
    let out = compile_tab_lazy(&src, &[step("2")], &tmpdir()).unwrap().collect().unwrap();
    assert_eq!(f64_col(&out, "Bal_Cr"), vec![Some(0.5), Some(1.0), Some(2.0)]);

    // Python export: the parsed number, or the same error raised (never 1.0).
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[step("1,000")], None);
    assert!(py.contains("/ 1000.0)"), "{py}");
    let py = micro_parquet::macros::generate_python_polars_code(std::path::Path::new("in.parquet"), true, &IngestionConfig::default(), &[step("abc")], None);
    assert!(py.contains("raise ValueError(\"Math column 'Bal_Cr': the value 'abc' is not a number"), "{py}");
}

#[test]
fn t29_pii_key_file() {
    let dir = tmpdir().join("t29_key");
    let _ = std::fs::remove_dir_all(&dir);
    let key_path = dir.join("portable_data").join("pii_key.bin");
    assert_eq!(read_pii_key(&key_path).unwrap(), None);

    // First use creates 32 random bytes; later uses read the same key.
    let (k, created) = load_or_create_pii_key(&key_path).unwrap();
    assert!(created);
    assert_eq!(std::fs::read(&key_path).unwrap(), k.to_vec());
    let (k_again, created) = load_or_create_pii_key(&key_path).unwrap();
    assert!(!created);
    assert_eq!(k, k_again);
    assert_ne!(k, [0u8; 32]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // Concurrent first use: every thread gets the same key.
    let race_path = dir.join("race").join("pii_key.bin");
    let keys: Vec<[u8; 32]> = (0..8)
        .map(|_| {
            let p = race_path.clone();
            std::thread::spawn(move || load_or_create_pii_key(&p).unwrap().0)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    assert!(keys.iter().all(|x| *x == keys[0]));
    let leftovers = std::fs::read_dir(race_path.parent().unwrap()).unwrap().count();
    assert_eq!(leftovers, 1, "temporary key files are removed");

    // Export, then import into another install: same fingerprint; the old key is backed up.
    let exported = dir.join("backup_key.bin");
    assert!(!export_pii_key(&key_path, &exported).unwrap());
    let other = dir.join("other").join("pii_key.bin");
    let (k_other, _) = load_or_create_pii_key(&other).unwrap();
    assert_ne!(pii_key_fingerprint(&k_other), pii_key_fingerprint(&k));
    let bak = import_pii_key(&other, &exported).unwrap().expect("backup of the replaced key");
    assert_eq!(std::fs::read(&bak).unwrap(), k_other.to_vec());
    assert_eq!(read_pii_key(&other).unwrap(), Some(k));

    // A file that is not a key is refused and changes nothing.
    let junk = dir.join("junk.bin");
    std::fs::write(&junk, b"not a key").unwrap();
    let err = import_pii_key(&other, &junk).err().expect("wrong size");
    assert!(format!("{err:#}").contains("expected 32"), "{err:#}");
    assert_eq!(read_pii_key(&other).unwrap(), Some(k));

    // A damaged key file is an error (never silently replaced).
    std::fs::write(&key_path, b"short").unwrap();
    let err = load_or_create_pii_key(&key_path).err().expect("damaged key");
    let msg = format!("{err:#}");
    assert!(msg.contains("damaged") && msg.contains("5 bytes"), "{msg}");
    assert_eq!(std::fs::read(&key_path).unwrap(), b"short".to_vec());
    assert_eq!(pii_key_fingerprint(&k).len(), 8);
}
