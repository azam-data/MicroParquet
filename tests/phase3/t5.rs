//! Phase 3 T5: the Column inspector's statistics (frame with nulls).

use micro_parquet::column_stats::*;
use polars::prelude::*;

fn day(s: &str) -> i32 {
    (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32
}

/// 12 rows. Balance: 10 values and 2 nulls. Opndat: 11 dates and a null. Brancd: DHK x5, CTG x3,
/// SYL x2, KHL x1, plus one blank ("  ") that is not a value.
fn frame() -> DataFrame {
    let opn = Series::new(
        "Opndat".into(),
        &[
            Some(day("2026-03-01")),
            Some(day("2026-01-15")),
            None,
            Some(day("2026-02-01")),
            Some(day("2026-01-15")),
            Some(day("2026-04-01")),
            Some(day("2026-03-01")),
            Some(day("2026-03-02")),
            Some(day("2026-03-03")),
            Some(day("2026-03-04")),
            Some(day("2026-03-05")),
            Some(day("2026-03-06")),
        ],
    )
    .cast(&DataType::Date)
    .unwrap();
    DataFrame::new(vec![
        Series::new(
            "Balance".into(),
            &[Some(100.0f64), Some(200.0), None, Some(300.0), Some(400.0), Some(500.0), Some(600.0), Some(700.0), None, Some(800.0), Some(900.0), Some(1000.0)],
        )
        .into(),
        Series::new(
            "Brancd".into(),
            &[Some("DHK"), Some("DHK"), Some("CTG"), Some("DHK"), Some("CTG"), Some("SYL"), Some("DHK"), Some("DHK"), Some("CTG"), Some("SYL"), Some("KHL"), Some("  ")],
        )
        .into(),
        opn.into(),
        Series::new("Count".into(), &[1i64, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4]).into(),
        Series::new("Flag".into(), &[true, false, true, true, false, true, true, false, true, true, false, true]).into(),
        Series::new("Same".into(), &[5.0f64; 12]).into(),
    ])
    .unwrap()
}

fn stats(column: &str) -> ColumnStats {
    compute_column_stats(frame().lazy(), column, 7).unwrap()
}

#[test]
fn t5_number_column_with_nulls() {
    let s = stats("Balance");
    assert_eq!(s.kind, ColumnKind::Number);
    assert_eq!(s.dtype, "f64");
    assert_eq!((s.rows, s.blanks), (12, 2));
    assert_eq!(s.key, 7);
    let n = s.number.unwrap();
    assert_eq!(n.sum, 5_500.0);
    assert_eq!(n.min, 100.0);
    assert_eq!(n.max, 1_000.0);
    assert_eq!(n.mean, 550.0, "5,500 over the 10 known values");
    assert_eq!(n.distinct, 10);
    assert!(s.date.is_none() && s.text.is_none());
}

#[test]
fn t5_histogram_bins_add_up_to_the_non_null_count() {
    let s = stats("Balance");
    let h = s.number.unwrap().histogram;
    assert_eq!(h.len(), HISTOGRAM_BINS);
    assert_eq!(h.iter().sum::<u64>(), 10, "10 known values");
    // 100..1000 in ten bins of 90: 100 and 190 | 200 | 300 | ... the maximum falls in the last bin
    assert_eq!(h, vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1].iter().map(|x: &u64| *x).collect::<Vec<u64>>());
    // integers: counts 1,1,2,2,2,3,3,3,3,4,4,4 → min 1, max 4
    let c = stats("Count");
    let h = c.number.unwrap().histogram;
    assert_eq!(h.iter().sum::<u64>(), 12);
    assert_eq!(h[0], 2, "the two 1s are in the first bin");
    assert_eq!(h[HISTOGRAM_BINS - 1], 3, "the three 4s (the maximum) are in the last bin");
    // all values equal: one bin holds them all
    let same = stats("Same").number.unwrap();
    assert_eq!(same.histogram.iter().sum::<u64>(), 12);
    assert_eq!(same.histogram[0], 12);
    assert_eq!((same.min, same.max, same.distinct), (5.0, 5.0, 1));
}

#[test]
fn t5_dates_show_min_and_max() {
    let s = stats("Opndat");
    assert_eq!(s.kind, ColumnKind::Date);
    assert_eq!((s.rows, s.blanks), (12, 1));
    let d = s.date.unwrap();
    assert_eq!(d.min, day("2026-01-15"));
    assert_eq!(d.max, day("2026-04-01"));
    assert!(s.number.is_none());
}

#[test]
fn t5_text_shows_distinct_count_and_the_top_five_with_counts() {
    let s = stats("Brancd");
    assert_eq!(s.kind, ColumnKind::Text);
    assert_eq!((s.rows, s.blanks), (12, 1), "the spaces-only value is a blank");
    let t = s.text.unwrap();
    assert_eq!(t.distinct, 4, "DHK, CTG, SYL, KHL (blank is not a value)");
    assert_eq!(
        t.top,
        vec![("DHK".to_string(), 5), ("CTG".to_string(), 3), ("SYL".to_string(), 2), ("KHL".to_string(), 1)]
    );
    // ties are listed in text order
    let df = DataFrame::new(vec![Series::new("T".into(), &["b", "a", "c", "a", "b", "z"]).into()]).unwrap();
    let t = compute_column_stats(df.lazy(), "T", 0).unwrap().text.unwrap();
    assert_eq!(t.top[0], ("a".to_string(), 2));
    assert_eq!(t.top[1], ("b".to_string(), 2));
    // more than five values: only five are listed
    let many: Vec<String> = (0..9).map(|i| format!("v{i}")).collect();
    let df = DataFrame::new(vec![Series::new("T".into(), many).into()]).unwrap();
    assert_eq!(compute_column_stats(df.lazy(), "T", 0).unwrap().text.unwrap().top.len(), TOP_VALUES);
}

#[test]
fn t5_other_types_and_missing_columns() {
    let s = stats("Flag");
    assert_eq!(s.kind, ColumnKind::Other);
    assert_eq!((s.rows, s.blanks), (12, 0));
    assert!(s.number.is_none() && s.date.is_none() && s.text.is_none());
    let err = compute_column_stats(frame().lazy(), "Nope", 0).err().unwrap();
    assert!(format!("{err:#}").contains("'Nope'"), "{err:#}");
}

#[test]
fn t5_an_all_blank_column_has_no_statistics_but_counts_its_blanks() {
    let df = DataFrame::new(vec![Series::new("X".into(), &[None::<f64>, None, None]).into()]).unwrap();
    let s = compute_column_stats(df.lazy(), "X", 0).unwrap();
    assert_eq!((s.rows, s.blanks), (3, 3));
    assert!(s.number.is_none(), "no values, no sum or histogram");
    let df = DataFrame::new(vec![Series::new("T".into(), &[Some(""), None, Some(" ")]).into()]).unwrap();
    let s = compute_column_stats(df.lazy(), "T", 0).unwrap();
    assert_eq!(s.blanks, 3);
    assert_eq!(s.text.unwrap().top, vec![]);
}

#[test]
fn t5_the_job_runs_over_the_whole_result_after_the_steps_and_carries_a_key() {
    use micro_parquet::jobs::JobCtx;
    use micro_parquet::models::*;
    use micro_parquet::tab_jobs::*;
    let dir = std::env::temp_dir().join(format!("mp_t5_job_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("loans.parquet");
    let mut df = frame();
    ParquetWriter::new(std::fs::File::create(&p).unwrap()).finish(&mut df).unwrap();
    let src = TabSource::SingleFile { path: p, is_parquet: true, config: IngestionConfig::default() };
    let run = |steps: Vec<TransformStep>, column: &str| {
        compute_tab_column_stats(
            ColumnStatsRequest { tab_id: 4, source: src.clone(), steps, column: column.into(), temp_dir: dir.clone() },
            &JobCtx::inert(),
        )
        .unwrap()
    };
    let r = run(vec![], "Balance");
    assert_eq!(r.tab_id, 4);
    assert_eq!(r.stats.rows, 12);
    assert_eq!(r.stats.key, stats_key(&[]));
    // after a filter the statistics follow the result, and the key changes
    let step = TransformStep::FilterRows { column: "Brancd".into(), operator: FilterOp::Eq, value: "DHK".into(), keep_matching: true };
    let r2 = run(vec![step.clone()], "Balance");
    assert_eq!(r2.stats.rows, 5);
    assert_eq!(r2.stats.number.as_ref().unwrap().sum, 100.0 + 200.0 + 300.0 + 600.0 + 700.0, "the five DHK rows");
    assert_ne!(r2.stats.key, r.stats.key, "a result for older steps can be told apart");
    assert_eq!(r2.stats.key, stats_key(&[step]));
    // a column the steps removed
    let err = compute_tab_column_stats(
        ColumnStatsRequest { tab_id: 4, source: src.clone(), steps: vec![], column: "Gone".into(), temp_dir: dir.clone() },
        &JobCtx::inert(),
    )
    .err()
    .unwrap();
    assert!(format!("{err:#}").contains("'Gone'"));
    std::fs::remove_dir_all(&dir).ok();
}
