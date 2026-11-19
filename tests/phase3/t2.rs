//! Phase 3 T2: multi-level sort (synthetic frame with ties and blanks).

use micro_parquet::engine::compile_tab_lazy;
use micro_parquet::macros::*;
use micro_parquet::models::*;
use polars::prelude::*;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mp_t2_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn day(s: &str) -> i32 {
    (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32
}

/// Seq 1..10. Rows 2 and 5 tie on every key; rows 7 and 8 have a blank branch ("" and null).
fn source(d: &std::path::Path) -> TabSource {
    let opn = Series::new(
        "Opndat".into(),
        &[
            Some(day("2026-03-01")),
            Some(day("2026-02-01")),
            Some(day("2026-01-15")),
            Some(day("2026-01-10")),
            Some(day("2026-02-01")),
            Some(day("2026-05-01")),
            Some(day("2026-01-01")),
            Some(day("2026-01-01")),
            None,
            Some(day("2026-01-01")),
        ],
    )
    .cast(&DataType::Date)
    .unwrap();
    let mut df = DataFrame::new(vec![
        Series::new("Seq".into(), &(1..=10).collect::<Vec<i64>>()).into(),
        Series::new(
            "Brancd".into(),
            &[Some("DHK"), Some("CTG"), Some("DHK"), Some("DHK"), Some("CTG"), Some("CTG"), Some(""), None, Some("DHK"), Some("CTG")],
        )
        .into(),
        Series::new(
            "Balance".into(),
            &[Some(500.0f64), Some(700.0), Some(900.0), Some(500.0), Some(700.0), None, Some(100.0), Some(100.0), Some(500.0), Some(700.0)],
        )
        .into(),
        opn.into(),
    ])
    .unwrap();
    let p = d.join("loans.parquet");
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() }
}

fn lv(column: &str, descending: bool) -> SortLevel {
    SortLevel { column: column.into(), descending, ..Default::default() }
}

fn order(src: &TabSource, d: &std::path::Path, levels: Vec<SortLevel>) -> Vec<i64> {
    let df = compile_tab_lazy(src, &[TransformStep::SortRows { levels }], d).unwrap().collect().unwrap();
    df.column("Seq").unwrap().i64().unwrap().into_iter().map(|v| v.unwrap()).collect()
}

#[test]
fn t2_three_levels_text_asc_number_desc_date_asc_with_ties_and_blanks() {
    let d = dir("three");
    let src = source(&d);
    let got = order(&src, &d, vec![lv("Brancd", false), lv("Balance", true), lv("Opndat", false)]);
    // CTG: 700s by date (10, then the tied 2 and 5 in source order), the blank balance (6) last;
    // DHK: 900 (3), then the 500s by date (4, 1, and 9 with a blank date last); blank branches last.
    assert_eq!(got, vec![10, 2, 5, 6, 3, 4, 1, 9, 7, 8]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t2_blanks_are_last_in_both_directions_and_empty_text_counts_as_blank() {
    let d = dir("blank");
    let src = source(&d);
    // highest branch first: DHK then CTG, but the blank branches ("" and null) are still last
    assert_eq!(order(&src, &d, vec![lv("Brancd", true), lv("Balance", true), lv("Opndat", false)]), vec![3, 4, 1, 9, 10, 2, 5, 6, 7, 8]);
    // balance highest first: the one blank balance (6) is last
    assert_eq!(order(&src, &d, vec![lv("Balance", true)]), vec![3, 2, 5, 10, 1, 4, 9, 7, 8, 6]);
    // balance lowest first: still last
    assert_eq!(order(&src, &d, vec![lv("Balance", false)]), vec![7, 8, 1, 4, 9, 2, 5, 10, 3, 6]);
    // dates: the blank date (9) last either way
    let asc = order(&src, &d, vec![lv("Opndat", false)]);
    let desc = order(&src, &d, vec![lv("Opndat", true)]);
    assert_eq!(*asc.last().unwrap(), 9);
    assert_eq!(*desc.last().unwrap(), 9);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t2_the_sort_is_stable_so_ties_keep_their_earlier_order() {
    let d = dir("stable");
    let src = source(&d);
    let g = order(&src, &d, vec![lv("Brancd", false)]);
    // within DHK (1,3,4,9) and CTG (2,5,6,10) the source order stays; blanks (7, 8) last
    assert_eq!(g, vec![2, 5, 6, 10, 1, 3, 4, 9, 7, 8]);
    // sorting twice by different keys: the second sort is the main key, the first the tie-break
    let first = TransformStep::SortRows { levels: vec![lv("Balance", false)] };
    let second = TransformStep::SortRows { levels: vec![lv("Brancd", false)] };
    let df = compile_tab_lazy(&src, &[first, second], &d).unwrap().collect().unwrap();
    let both: Vec<i64> = df.column("Seq").unwrap().i64().unwrap().into_iter().map(|v| v.unwrap()).collect();
    assert_eq!(both, vec![2, 5, 10, 6, 1, 4, 9, 3, 7, 8]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t2_no_levels_changes_nothing_and_a_missing_column_is_named() {
    let d = dir("errors");
    let src = source(&d);
    assert_eq!(order(&src, &d, vec![]), (1..=10).collect::<Vec<i64>>());
    let err = compile_tab_lazy(&src, &[TransformStep::SortRows { levels: vec![lv("Nope", false)] }], &d)
        .and_then(|lf| Ok(lf.collect()?))
        .err()
        .unwrap();
    assert!(format!("{err:#}").contains("'Nope'"), "{err:#}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn t2_header_marks_show_level_and_direction() {
    let steps = vec![
        TransformStep::SortRows { levels: vec![lv("Old", false)] },
        TransformStep::RenameColumn { old_name: "a".into(), new_name: "b".into() },
        TransformStep::SortRows { levels: vec![lv("Brancd", false), lv("Balance", true)] },
    ];
    let marks = sort_marks(&steps);
    assert_eq!(marks.len(), 2, "the last sort step decides");
    assert_eq!(sort_mark_label(marks["Brancd"].0, marks["Brancd"].1), "1 \u{2191}");
    assert_eq!(sort_mark_label(marks["Balance"].0, marks["Balance"].1), "2 \u{2193}");
    assert!(sort_marks(&[]).is_empty());
    assert_eq!(steps[2].short_label(), "Sort: Brancd \u{2191}, Balance \u{2193}");
}

#[test]
fn t2_sort_step_serialises_with_defaults_and_translates_to_python() {
    let s: TransformStep = serde_json::from_str(r#"{"SortRows":{"levels":[{"column":"A"},{"column":"B","descending":true}]}}"#).unwrap();
    assert_eq!(s.sort_levels().unwrap(), &[lv("A", false), lv("B", true)]);
    let bare: TransformStep = serde_json::from_str(r#"{"SortRows":{}}"#).unwrap();
    assert!(bare.sort_levels().unwrap().is_empty());
    let py = generate_python_polars_code(std::path::Path::new("x.parquet"), true, &IngestionConfig::default(), &[s.clone()], None).unwrap();
    assert!(py.contains("descending=[False, True], nulls_last=True, maintain_order=True"), "{py}");
    assert!(python_untranslated_steps(&[s]).is_empty());
}
