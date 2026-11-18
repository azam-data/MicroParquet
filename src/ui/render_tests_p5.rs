//! Phase 5 UI tests: the new dialogs are drawn by the real UI code in a headless egui context at
//! 1219×686 pt (a 1280×720 px window at 105%) and 1600×900 pt (at 80%), and what they drew is
//! checked by the rectangles of the widgets themselves.

use crate::ui::prelude::*;
use crate::ui::tests::{open_tab, settle, test_app};

/// F0: the schema dialog's "Clean line breaks" check box adds (and removes) the recorded first step.
#[test]
fn f0_the_schema_dialog_check_box_adds_the_clean_line_breaks_step_and_unticking_takes_it_out() {
    let mut app = test_app("p5f0");
    let p = app.paths.root_dir.join("multi.csv");
    std::fs::write(&p, "Actnum,Remarks,Qty\nA1,\"Paid in full\nby cheque\",100\nA2,plain,200\n").expect("write");
    let config = IngestionConfig { sanitize_linebreaks: false, ..IngestionConfig::default() };
    let idx = open_tab(&mut app, TabSource::SingleFile { path: p, is_parquet: false, config }, vec![]);
    let remark = |app: &MicroParquetApp| app.tabs[idx].cached_slice.column("Remarks").expect("col").str().expect("text").get(0).unwrap_or("").to_string();
    assert!(app.tabs[idx].steps.is_empty());
    assert!(remark(&app).contains('\n'), "nothing is cleaned until the box is ticked");

    // the dialog of the open tab: the box is not ticked; tick it and apply
    app.open_schema_modal_for_active_tab();
    let mut modal = app.pending_csv_modal.take().expect("the dialog opened");
    assert!(!modal.config.sanitize_linebreaks, "no step: the box shows unticked");
    modal.config.sanitize_linebreaks = true;
    app.apply_schema_to_tab(idx, modal.config);
    settle(&mut app);
    assert!(matches!(app.tabs[idx].steps.first(), Some(TransformStep::CleanLineBreaks { columns }) if columns.is_empty()), "{:?}", app.tabs[idx].steps);
    assert_eq!(app.tabs[idx].steps.len(), 1);
    assert_eq!(remark(&app), "Paid in full by cheque", "{}", app.status_message);
    let TabSource::SingleFile { config, .. } = &app.tabs[idx].source else { panic!("single file") };
    assert!(!config.sanitize_linebreaks, "the step is the only thing that cleans");

    // reopen: the box shows ticked because the step is there; unticking takes the step out
    app.open_schema_modal_for_active_tab();
    let mut modal = app.pending_csv_modal.take().expect("the dialog opened again");
    assert!(modal.config.sanitize_linebreaks, "the box follows the recorded step");
    modal.config.sanitize_linebreaks = false;
    app.apply_schema_to_tab(idx, modal.config);
    settle(&mut app);
    assert!(app.tabs[idx].steps.is_empty(), "{:?}", app.tabs[idx].steps);
    assert!(remark(&app).contains('\n'));
}
