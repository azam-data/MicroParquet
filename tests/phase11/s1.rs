//! Phase 11 items 4 and 5: keep or drop a column, and reverse the sign of a number column, in the schema.

use crate::common::{dir, floats, strings};
use micro_parquet::engine::{compile_tab_lazy, detect_file_schema};
use micro_parquet::models::*;
use polars::prelude::*;

fn csv_tab(name: &str, edit: impl Fn(&mut IngestionConfig)) -> (std::path::PathBuf, anyhow::Result<DataFrame>) {
    let d = dir(name);
    let p = d.join("adv.csv");
    std::fs::write(&p, "Branch,Advance,Note,Count\nA,-100.5,x,3\nB,0,y,4\nC,-20,z,5\n").expect("csv");
    let mut cfg = detect_file_schema(&p).expect("schema");
    edit(&mut cfg);
    let src = TabSource::SingleFile { path: p, is_parquet: false, config: cfg };
    let out = compile_tab_lazy(&src, &[], &d).and_then(|lf| Ok(lf.collect()?));
    (d, out)
}

#[test]
fn s1_a_dropped_column_is_not_in_the_table_and_the_others_keep_their_order() {
    let (_, out) = csv_tab("p11s1_a", |c| c.columns[2].drop = true);
    let df = out.expect("loads");
    assert_eq!(df.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), vec!["Branch", "Advance", "Count"]);
    assert_eq!(df.height(), 3);
}

#[test]
fn s1_b_reverse_sign_flips_every_value_and_zero_stays_zero() {
    let (_, out) = csv_tab("p11s1_b", |c| c.columns[1].reverse_sign = true);
    let df = out.expect("loads");
    let v = floats(&df, "Advance");
    assert_eq!(v, vec![Some(100.5), Some(0.0), Some(20.0)]);
    assert!(v[1].expect("zero").is_sign_positive(), "zero must not become -0");
}

#[test]
fn s1_c_reverse_sign_on_a_text_column_stops_with_the_reason() {
    let (_, out) = csv_tab("p11s1_c", |c| c.columns[0].reverse_sign = true);
    let e = format!("{:#}", out.expect_err("refused"));
    assert!(e.contains("Reverse sign") && e.contains("Branch"), "{e}");
}

#[test]
fn s1_d_leaving_out_every_column_stops_with_the_reason() {
    let (_, out) = csv_tab("p11s1_d", |c| c.columns.iter_mut().for_each(|x| x.drop = true));
    let e = format!("{:#}", out.expect_err("refused"));
    assert!(e.contains("at least one column"), "{e}");
}

#[test]
fn s1_e_a_dropped_column_may_share_a_name_and_is_not_checked_for_dr_cr() {
    let (_, out) = csv_tab("p11s1_e", |c| {
        c.columns[2].drop = true;
        c.columns[1].rename_to = Some("Note".into()); // the dropped column's name is free
    });
    let df = out.expect("loads");
    assert_eq!(strings(&df, "Branch")[0].as_deref(), Some("A"));
    assert_eq!(floats(&df, "Note")[0], Some(-100.5));
}

#[test]
fn s1_f_old_schema_json_without_the_new_fields_loads_with_every_column_kept() {
    let json = r#"{"name":"a","dtype":"Float64","pii_mask":"None","sample_values":[]}"#;
    let c: ColumnSchemaConfig = serde_json::from_str(json).expect("old json");
    assert!(!c.drop && !c.reverse_sign);
}

#[test]
fn s1_g_parquet_reverse_sign_and_drop() {
    let d = dir("p11s1_g");
    let df = DataFrame::new(vec![Series::new("Branch".into(), &["A", "B"]).into(), Series::new("Adv".into(), &[-5.0f64, 7.5]).into(), Series::new("Junk".into(), &[1i64, 2]).into()]).expect("df");
    let src = crate::common::parquet(&d, "p", &df);
    let TabSource::SingleFile { path, .. } = &src else { panic!("single file") };
    let mut cfg = detect_file_schema(path).expect("schema");
    for c in cfg.columns.iter_mut() {
        if c.name == "Adv" {
            c.reverse_sign = true;
        }
        if c.name == "Junk" {
            c.drop = true;
        }
    }
    let src = TabSource::SingleFile { path: path.clone(), is_parquet: true, config: cfg };
    let out = compile_tab_lazy(&src, &[], &d).expect("compiles").collect().expect("collects");
    assert_eq!(out.get_column_names().iter().map(|n| n.to_string()).collect::<Vec<_>>(), vec!["Branch", "Adv"]);
    assert_eq!(floats(&out, "Adv"), vec![Some(5.0), Some(-7.5)]);
}
