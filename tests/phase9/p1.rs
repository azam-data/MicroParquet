//! P1: the pop-out pivot builder. The window (Pop out, Dock, the same state in both places, Esc, the larger preview, the place
//! the window opens) is tested in `src/ui/render_tests_p9.rs` (`p1_…`); here are the parts under it: the saved rectangle is a
//! setting that old files read as none and that leaves old files' bytes alone, and the preview brings back as many rows as it
//! is asked for, never fewer than the docked dialog's 20.

use crate::common::{dir, parquet};
use micro_parquet::jobs::JobCtx;
use micro_parquet::models::*;
use micro_parquet::settings::Settings;
use micro_parquet::tab_jobs::*;
use polars::prelude::*;

#[test]
fn p1_the_saved_window_rectangle_is_none_in_old_files_round_trips_and_leaves_old_bytes_alone() {
    let d = dir("p9p1_settings");
    // a Phase 8 settings.json (without the field) reads with none, and is written back byte for byte
    let old = Settings { right_panel_width: Some(480.0), recent_projects: vec!["a.mpproj".into()], ..Default::default() };
    old.save(&d).expect("saves");
    let text = std::fs::read_to_string(d.join("settings.json")).expect("the file");
    assert!(!text.contains("pivot_popout_rect"), "a setting that was never made is not written: {text}");
    let (back, note) = Settings::load(&d);
    assert!(note.is_none());
    assert_eq!(back.pivot_popout_rect, None);
    assert_eq!(back, old);
    back.save(&d).expect("saves again");
    assert_eq!(std::fs::read_to_string(d.join("settings.json")).expect("the file"), text, "the same bytes");
    // a rectangle is kept
    let with = Settings { pivot_popout_rect: Some([120.0, 80.0, 1280.0, 800.0]), ..back };
    with.save(&d).expect("saves");
    let (again, _) = Settings::load(&d);
    assert_eq!(again.pivot_popout_rect, Some([120.0, 80.0, 1280.0, 800.0]));
    // a file that names the field with null reads as none
    std::fs::write(d.join("settings.json"), r#"{"pivot_popout_rect": null}"#).expect("writes");
    assert_eq!(Settings::load(&d).0.pivot_popout_rect, None);
}

fn book(groups: usize) -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), (0..groups).map(|i| format!("B{:03}", i + 1)).collect::<Vec<_>>()).into(),
        Series::new("Balance".into(), (0..groups).map(|i| 1000.0 + i as f64).collect::<Vec<_>>()).into(),
    ])
    .expect("book")
}

fn preview(d: &std::path::Path, groups: usize, rows: usize) -> PivotPreviewResult {
    let source = parquet(d, "book", &book(groups));
    let spec = PivotSpec { rows: vec!["Branch".into()], values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }], row_totals: true, column_totals: true, ..Default::default() };
    let req = PivotPreviewRequest { tab_id: 1, token: 7, source, steps: vec![], spec, temp_dir: d.to_path_buf(), rows };
    compute_pivot_preview(req, &JobCtx::inert()).expect("the preview")
}

#[test]
fn p1_the_preview_brings_back_the_rows_asked_for_and_never_fewer_than_the_docked_twenty() {
    let d = dir("p9p1_rows");
    // 600 branches and the grand total row: 601 rows in the pivot
    for (asked, want) in [(20, 20), (0, 20), (5, 20), (60, 60), (140, 140), (500, 500), (100_000, 500)] {
        let r = preview(&d, 600, asked);
        let t = r.table.expect("a table");
        assert_eq!(t.height(), want, "asked for {asked}");
        assert_eq!(r.limit, want, "the limit says how far it was cut, for {asked}");
        assert_eq!(r.total_rows, 601, "every row of the pivot is counted, for {asked}");
        // the rows are the first ones, in the pivot's order
        let first = t.column("Branch").expect("branch").str().expect("text").get(0).map(str::to_string);
        assert_eq!(first.as_deref(), Some("B001"));
    }
    // a pivot with fewer rows than asked gives them all
    let r = preview(&d, 30, 140);
    assert_eq!(r.table.expect("a table").height(), 31, "30 branches and the total");
    assert_eq!(r.limit, 140);
}
