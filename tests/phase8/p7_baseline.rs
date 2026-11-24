//! The Phase 7 baseline of goal 3: sample macros, a pack and a saved chart, each made and saved by the
//! Phase 7 code (`main` at f3e6f91, in a temporary worktree), and the blake3 hash of what that code gave.
//! `tests/fixtures/phase7_replay.hashes.json` holds the hashes; `w0_the_phase7_samples_replay…` loads every
//! file with today's code and compares. This file uses only APIs that Phase 7 had, so the same code made
//! the hashes. Do not edit the fixtures by hand.

#![allow(dead_code)]

use micro_parquet::models::*;
use micro_parquet::packs::*;
use polars::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const LOANS: &str = "ACNO,BAL,RATE,MAT,CAT,GRADE,TENOR\nL1,1000000,9.5,2027-03-31,Continuous,STD-0,1Y\nL2,2000000,12,2028-10-31,Demand,SS,3M\nL3,500000,8,2026-12-15,Fixed term,B/L,6M\nL4,750000,10.5,2026-11-30,Demand,STD-0,O/N\n";
pub const DEPOSITS: &str = "ACNO,BAL,MAT,PROD\nD1,3000000,2026-12-31,SB\nD2,1500000,2027-06-30,TD\nD3,500000,2026-11-30,TD\nD4,250000,2027-01-31,CD\n";

/// The macros of the baseline: (file stem, steps as JSON).
pub fn macros() -> Vec<(&'static str, serde_json::Value)> {
    let bucket = serde_json::to_value(&CustomBucketDef::default_presets()[0]).expect("bucket preset");
    let band = serde_json::json!({"name": "1pt", "mode": {"Fixed": {"start_pct": 0.0, "end_pct": 15.0, "step_pct": 1.0}}, "lower_inclusive": false});
    let tenor = serde_json::json!({"ConditionalColumn": {"new_col": "Tenor", "rules": [
        {"col_name": "Int Rate", "op": "Gt", "compare_val": "10", "output_val": "1Y"},
        {"col_name": "Int Rate", "op": "Gt", "compare_val": "9", "output_val": "6M"}], "fallback": "3M", "output_as_float": false}});
    vec![
        (
            "phase7_rows",
            serde_json::json!([
                {"FilterGroup": {"conditions": [{"column": "Brancd", "op": "InList", "value": "DHK, CTG, SYL", "values": ["DHK", "CTG", "SYL"]}], "join": "And", "keep_matching": true}},
                tenor,
                {"SortRows": {"levels": [
                    {"column": "Tenor", "descending": false, "order": "Tenor"},
                    {"column": "Brancd", "descending": false, "order": {"List": {"name": "Branches", "values": ["SYL", "DHK", "CTG"], "case_insensitive": true}}},
                    {"column": "Balance", "descending": true}]}},
                {"MathFormula": {"new_col": "Bal_Cr", "left_col": "Balance", "op": "Div", "right_is_col": false, "right_val_or_col": "10000000"}},
                {"NormalizeEntity": {"source_col": "Acttit", "new_col": "Entity"}},
                {"MaturityBucket": {"mat_col": "Matdat", "cutoff_date": "2026-03-31", "bucket_def": bucket, "null_as_sight": true, "new_col": "Bucket"}},
                {"RateBand": {"rate_col": "Int Rate", "band_def": band, "new_col": "Rate Band"}},
                {"RateShockSim": {"bal_col": "Balance", "rate_col": "Int Rate", "mat_col": "Matdat", "cutoff_date": "2026-03-31", "shock_bps": 200.0, "side_col": "Side"}},
                {"LoanLimitUtilization": {"limit_col": "Sanctioned_Limit", "bal_col": "Balance", "ccf_pct": 20.0}},
                {"FtpCurveMap": {"opndat_col": "Opndat", "matdat_col": "Matdat", "bal_col": "Balance", "rate_col": "Int Rate",
                    "curve": [{"tenor_days": 90, "ftp_rate": 7.0}, {"tenor_days": 365, "ftp_rate": 8.0}, {"tenor_days": 1825, "ftp_rate": 9.5}],
                    "is_liability": false, "interpolation": "Linear"}}
            ]),
        ),
        (
            "phase7_pivot_bands",
            serde_json::json!([
                {"RateBand": {"rate_col": "Int Rate", "band_def": band, "new_col": "Rate Band"}},
                {"PivotTable": {"rows": ["Rate Band"], "columns": ["Brancd"], "values": [{"field": "Balance", "agg": "Sum"}],
                    "field_orders": [{"field": "Brancd", "order": {"List": {"name": "Branches", "values": ["SYL", "DHK", "CTG", "XXX"], "case_insensitive": true}}}]}}
            ]),
        ),
        (
            "phase7_pivot_tenor",
            serde_json::json!([
                tenor,
                {"PivotTable": {"rows": ["Tenor"], "values": [{"field": "Balance", "agg": "Sum"}, {"field": "Balance", "agg": "Count"}],
                    "field_orders": [{"field": "Tenor", "order": "Tenor"}], "subtotals": false}}
            ]),
        ),
    ]
}

pub fn steps_of(v: &serde_json::Value) -> Vec<TransformStep> {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("steps: {e}"))
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

/// The pack of the baseline: a table with a slot step, a pivot and a gap ladder run.
pub fn pack() -> serde_json::Value {
    let preset = CustomBucketDef::default_presets()[0].name.clone();
    serde_json::json!({
        "name": "p7replay",
        "inputs": [
            {"slot": "Loans", "required_columns": ["ACNO", "BAL", "RATE", "MAT", "TENOR"],
             "steps": [{"SortRows": {"levels": [{"column": "TENOR", "descending": false, "order": "Tenor"}]}}]},
            {"slot": "Deposits", "required_columns": ["ACNO", "BAL", "MAT", "PROD"]}
        ],
        "items": [
            {"title": "Loans by tenor", "sheet": "Loans", "source": {"kind": "steps", "slot": "Loans"}},
            {"title": "Deposits by product", "sheet": "Dep", "source": {"kind": "pivot", "slot": "Deposits", "pivot":
                {"PivotTable": {"rows": ["PROD"], "values": [{"field": "BAL", "agg": "Sum"}],
                 "field_orders": [{"field": "PROD", "order": {"List": {"name": "Products", "values": ["TD", "SB", "CD"], "case_insensitive": true}}}]}}}},
            {"title": "Gap", "sheet": "Gap", "source": {"kind": "run", "run": {"tool": "gap_ladder",
                "inputs": [
                    {"source": "Loans", "spec": {"role": "Asset", "amount_col": "BAL", "maturity_col": "MAT"}, "product_col": ""},
                    {"source": "Deposits", "spec": {"role": "Liability", "amount_col": "BAL", "maturity_col": "MAT"}, "product_col": ""}],
                "preset": preset, "cutoff": "2026-10-31", "null_as_sight": true, "behaviour_file": "", "stress_tdrr": false}}}
        ],
        "output": {}
    })
}

/// The saved chart of the baseline (on the deposits table).
pub fn chart() -> serde_json::Value {
    serde_json::json!({"category": "PROD", "value": "BAL", "kind": "Bar", "order": {"List": {"name": "Products", "values": ["TD", "SB", "CD"], "case_insensitive": true}}})
}

pub struct Rig {
    pub root: PathBuf,
    pub paths: PortablePaths,
    pub spec: PackRunSpec,
}

pub fn rig(root: &Path) -> Rig {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root.join("temp")).expect("temp");
    let paths = PortablePaths { root_dir: root.to_path_buf(), macros_dir: root.join("macros"), schemas_dir: root.join("schemas"), curves_dir: root.join("curves"), buckets_file: root.join("buckets.json"), temp_dir: root.join("temp") };
    let (loans, deposits) = (root.join("loans.csv"), root.join("deposits.csv"));
    std::fs::write(&loans, LOANS).expect("loans");
    std::fs::write(&deposits, DEPOSITS).expect("deposits");
    let spec = PackRunSpec { files: BTreeMap::from([("Loans".to_string(), loans), ("Deposits".to_string(), deposits)]), cutoff: "2026-10-31".to_string() };
    Rig { root: root.to_path_buf(), paths, spec }
}

/// Runs a pack and hashes each sheet's table: (sheet, hash).
pub fn pack_hashes(pack: &ReportPack, rig: &Rig) -> Vec<(String, String)> {
    let r = run_pack(pack, &rig.spec, &rig.paths, "2026-11-23 10:00:00", &micro_parquet::jobs::JobCtx::inert()).unwrap_or_else(|e| panic!("pack: {e:#}"));
    r.sheets.iter().map(|s| (s.sheet.clone(), hash(&csv_of(&s.df)))).collect()
}

/// The chart's numbers on the deposits file, hashed.
pub fn chart_hash(spec: &micro_parquet::charts::spec::ChartSpec, rig: &Rig) -> String {
    let lf = LazyCsvReader::new(rig.root.join("deposits.csv")).with_has_header(true).finish().expect("deposits");
    let d = micro_parquet::charts::calc::compute(lf, spec).unwrap_or_else(|e| panic!("chart: {e:#}"));
    hash(&format!("{:?}|{:?}|{:?}|{:?}|{}", d.categories, d.series, d.values, d.rows, d.total_rows))
}
