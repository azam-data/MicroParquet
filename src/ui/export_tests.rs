//! End-to-end tests of the exports (L1, L3): each kind, on a plain tab and on a pivot tab, driven
//! through the same functions the menus and dialogs call. The save dialog is answered by a test hook
//! that returns a path WITHOUT an extension, as a user typing a bare name would: the file must still
//! get its extension and the right signature.

use crate::ui::prelude::*;
use crate::ui::save_dialog::{set_folder_answer, set_save_hook, SaveSpec};
use crate::ui::tests::{open_tab, parquet_source, settle, test_app};
use micro_parquet::commands::CommandId;
use polars::prelude::{CsvReadOptions, NamedFrom, SerReader, Series};
use std::cell::RefCell;
use std::rc::Rc;

fn frame() -> DataFrame {
    DataFrame::new(vec![
        Series::new("Branch".into(), &["Dhaka", "Chattogram", "Sylhet"]),
        Series::new("Balance".into(), &[3478848.7299999995f64, 224703214160.54498, 12.5]), // not a size: test data
        Series::new("Qty".into(), &[1i64, 22, 333]),
    ])
    .expect("frame")
}

fn pivot_step() -> TransformStep {
    TransformStep::from_pivot_spec(micro_parquet::models::PivotSpec {
        rows: vec!["Branch".into()],
        values: vec![PivotValue { field: "Balance".into(), agg: PivotAgg::Sum, weight_col: None, time: None }],
        column_totals: true,
        ..Default::default()
    })
}

/// A tab (plain or pivot) in a fresh app, and the folder the exports go to.
fn app_with(name: &str, pivot: bool) -> (MicroParquetApp, PathBuf, usize) {
    let mut app = test_app(name);
    let src = parquet_source(&app.paths.root_dir.clone(), "book", frame());
    let steps = if pivot { vec![pivot_step()] } else { vec![] };
    open_tab(&mut app, src, steps);
    let out = app.paths.root_dir.join("out");
    std::fs::create_dir_all(&out).expect("out");
    let rows = app.tabs[app.active_tab].total_rows;
    (app, out, rows)
}

/// Answers the save dialog with `<dir>/<default name without its extension>` and records what it was asked.
fn answer_saves(dir: &Path) -> Rc<RefCell<Vec<SaveSpec>>> {
    let asked: Rc<RefCell<Vec<SaveSpec>>> = Rc::default();
    let (log, dir) = (asked.clone(), dir.to_path_buf());
    set_save_hook(Some(Box::new(move |spec: &SaveSpec| {
        log.borrow_mut().push(spec.clone());
        let stem = Path::new(&spec.default_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Some(dir.join(stem))
    })));
    asked
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).expect("dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
    v.sort();
    v
}

fn only_file(dir: &Path) -> PathBuf {
    let f = files_in(dir);
    assert_eq!(f.len(), 1, "exactly one file (and no temporary leftover): {f:?}");
    f.into_iter().next().expect("one")
}

fn finish(app: &mut MicroParquetApp) {
    settle(app);
    settle(app);
}

fn read_csv(p: &Path) -> DataFrame {
    CsvReadOptions::default().with_has_header(true).try_into_reader_with_file_path(Some(p.to_path_buf())).expect("reader").finish().expect("the CSV parses back")
}

fn tab_frame(app: &MicroParquetApp) -> DataFrame {
    let t = &app.tabs[app.active_tab];
    micro_parquet::engine::compile_tab_lazy(&t.source, &t.steps, &app.paths.temp_dir).expect("lf").collect().expect("collect")
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
}

#[test]
fn l1_export_to_csv_writes_a_csv_with_its_extension_on_a_plain_tab_and_a_pivot_tab() {
    for pivot in [false, true] {
        let (mut app, out, _) = app_with("l1csv", pivot);
        let asked = answer_saves(&out);
        app.run_command(&egui::Context::default(), CommandId::ExportCsv);
        finish(&mut app);
        set_save_hook(None);
        let asked = asked.borrow();
        assert_eq!(asked.len(), 1, "pivot {pivot}: one save dialog");
        assert_eq!(asked[0].ext, "csv");
        assert!(asked[0].filter_name.contains("csv") && asked[0].default_name.ends_with(".csv"), "{:?}", asked[0]);
        let file = only_file(&out);
        assert_eq!(ext(&file), "csv", "pivot {pivot}: {file:?}");
        let bytes = std::fs::read(&file).expect("read");
        assert!(bytes.iter().all(|b| *b == b'\n' || *b == b'\r' || *b == b'\t' || (32..127).contains(b) || *b >= 128), "text, not binary");
        assert!(!bytes.starts_with(b"PAR1") && !bytes.starts_with(b"PK"));
        let back = read_csv(&file);
        let want = tab_frame(&app);
        assert_eq!(back.height(), want.height(), "pivot {pivot}: same rows");
        assert_eq!(back.get_column_names(), want.get_column_names(), "pivot {pivot}: same columns");
        // the numbers are written with at most 15 significant digits (K3)
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("3478848.73") && !text.contains("3478848.7299999995"), "{text}");
    }
}

#[test]
fn l1_export_to_parquet_writes_a_parquet_with_its_extension_on_a_plain_tab_and_a_pivot_tab() {
    for pivot in [false, true] {
        let (mut app, out, _) = app_with("l1pq", pivot);
        let asked = answer_saves(&out);
        app.run_command(&egui::Context::default(), CommandId::ExportParquet);
        finish(&mut app);
        set_save_hook(None);
        assert_eq!(asked.borrow().len(), 1);
        assert_eq!(asked.borrow()[0].ext, "parquet");
        let file = only_file(&out);
        assert_eq!(ext(&file), "parquet", "{file:?}");
        let bytes = std::fs::read(&file).expect("read");
        assert!(bytes.starts_with(b"PAR1") && bytes.ends_with(b"PAR1"), "pivot {pivot}: Parquet signature");
        let back = polars::prelude::LazyFrame::scan_parquet(&file, Default::default()).expect("scan").collect().expect("collect");
        let want = tab_frame(&app);
        assert_eq!((back.height(), back.get_column_names()), (want.height(), want.get_column_names()));
    }
}

#[test]
fn l1_export_to_excel_writes_an_xlsx_with_its_extension_on_a_plain_tab_and_a_pivot_tab() {
    for pivot in [false, true] {
        let (mut app, out, _) = app_with("l1xl", pivot);
        let asked = answer_saves(&out);
        app.export.multi_checks = vec![true; app.tabs.len()];
        app.run_guarded_export(GuardedExport::MultiXlsx);
        finish(&mut app);
        set_save_hook(None);
        assert_eq!(asked.borrow()[0].ext, "xlsx");
        let file = only_file(&out);
        assert_eq!(ext(&file), "xlsx", "{file:?}");
        let bytes = std::fs::read(&file).expect("read");
        assert!(bytes.starts_with(b"PK"), "pivot {pivot}: a zip (xlsx) signature");
        assert!(!bytes.starts_with(b"PAR1"));
    }
}

#[test]
fn l1_export_to_pdf_asks_the_options_first_and_writes_a_pdf_with_its_extension() {
    for pivot in [false, true] {
        let (mut app, out, _) = app_with("l1pdf", pivot);
        let asked = answer_saves(&out);
        app.export.multi_checks = vec![true; app.tabs.len()];
        app.run_guarded_export(GuardedExport::MultiPdf);
        finish(&mut app);
        assert!(app.export.pdf_options.is_some(), "the options dialog comes before the save dialog");
        assert!(asked.borrow().is_empty(), "no save dialog yet");
        app.confirm_pdf_options();
        finish(&mut app);
        set_save_hook(None);
        assert_eq!(asked.borrow().len(), 1);
        assert_eq!(asked.borrow()[0].ext, "pdf");
        let file = only_file(&out);
        assert_eq!(ext(&file), "pdf", "{file:?}");
        assert!(std::fs::read(&file).expect("read").starts_with(b"%PDF"), "pivot {pivot}: PDF signature");
    }
}

#[test]
fn l1_split_export_writes_every_file_with_its_extension_on_a_plain_tab_and_a_pivot_tab() {
    for pivot in [false, true] {
        for (format, ex, sig) in [(SplitExportFormat::Csv, "csv", None), (SplitExportFormat::Parquet, "parquet", Some(&b"PAR1"[..])), (SplitExportFormat::Xlsx, "xlsx", Some(&b"PK"[..]))] {
            let (mut app, out, rows) = app_with("l1split", pivot);
            set_folder_answer(Some(out.clone()));
            app.export.split_col = "Branch".to_string();
            app.export.split_format = format;
            app.run_guarded_export(GuardedExport::Split);
            finish(&mut app);
            set_folder_answer(None);
            let files = files_in(&out);
            let data: Vec<&PathBuf> = files.iter().filter(|p| p.file_name().is_some_and(|n| n != "split_manifest.csv")).collect();
            assert_eq!(data.len(), rows, "pivot {pivot} {ex}: one file per branch: {files:?}");
            for f in &data {
                assert_eq!(ext(f), ex, "{f:?}");
                if let Some(s) = sig {
                    assert!(std::fs::read(f).expect("read").starts_with(s), "{f:?} signature");
                }
            }
            let manifest = out.join("split_manifest.csv");
            assert!(manifest.exists(), "the manifest is written");
            assert_eq!(read_csv(&manifest).height(), rows);
            if format == SplitExportFormat::Csv {
                let total: usize = data.iter().map(|f| read_csv(f).height()).sum();
                assert_eq!(total, rows, "the files hold every row");
            }
        }
    }
}

#[test]
fn l1_a_path_without_the_extension_gets_it_and_the_dialogs_offer_only_their_file_type() {
    // the hook above returns bare names, so every test already proves the append; this checks the filter text
    let (mut app, out, _) = app_with("l1filter", false);
    let asked = answer_saves(&out);
    app.run_command(&egui::Context::default(), CommandId::ExportCsv);
    finish(&mut app);
    app.run_command(&egui::Context::default(), CommandId::ExportParquet);
    finish(&mut app);
    set_save_hook(None);
    let asked = asked.borrow();
    assert_eq!(asked.iter().map(|s| s.ext.as_str()).collect::<Vec<_>>(), vec!["csv", "parquet"]);
    assert!(asked.iter().all(|s| s.filter_name.contains(&format!("*.{}", s.ext))), "{asked:?}");
}

#[test]
fn l3_the_suggested_names_follow_the_tab_title_or_the_pdf_title() {
    let (mut app, out, _) = app_with("l3", true);
    // a pivot tab is named after its first row field in the app
    let idx = app.active_tab;
    app.tabs[idx].title = "Pivot: Branch".to_string();
    // Excel, one tab: its title, sanitised
    let asked = answer_saves(&out);
    app.export.multi_checks = vec![true; app.tabs.len()];
    app.run_guarded_export(GuardedExport::MultiXlsx);
    finish(&mut app);
    assert_eq!(asked.borrow().last().map(|s| s.default_name.clone()).as_deref(), Some("Pivot_ Branch.xlsx"));
    // PDF without a title: the tab's title
    app.run_guarded_export(GuardedExport::MultiPdf);
    finish(&mut app);
    app.confirm_pdf_options();
    finish(&mut app);
    assert_eq!(asked.borrow().last().map(|s| s.default_name.clone()).as_deref(), Some("Pivot_ Branch.pdf"));
    // PDF with a title: the title
    app.run_guarded_export(GuardedExport::MultiPdf);
    finish(&mut app);
    if let Some(o) = app.export.pdf_options.as_mut() {
        o.title = "ALCO pack: Nov 2026".to_string();
    }
    app.confirm_pdf_options();
    finish(&mut app);
    assert_eq!(asked.borrow().last().map(|s| s.default_name.clone()).as_deref(), Some("ALCO pack_ Nov 2026.pdf"));
    // several tabs ticked: "<first tab title> and N more" for Excel, and for a PDF without a title
    let src = parquet_source(&app.paths.root_dir.clone(), "book2", frame());
    open_tab(&mut app, src.clone(), vec![]);
    open_tab(&mut app, src, vec![]);
    assert_eq!(app.tabs.len(), 3);
    app.export.multi_checks = vec![true; 3];
    app.run_guarded_export(GuardedExport::MultiXlsx);
    finish(&mut app);
    assert_eq!(asked.borrow().last().map(|s| s.default_name.clone()).as_deref(), Some("Pivot_ Branch and 2 more.xlsx"));
    app.run_guarded_export(GuardedExport::MultiPdf);
    finish(&mut app);
    if let Some(o) = app.export.pdf_options.as_mut() {
        o.title = String::new();
    }
    app.confirm_pdf_options();
    finish(&mut app);
    set_save_hook(None);
    assert_eq!(asked.borrow().last().map(|s| s.default_name.clone()).as_deref(), Some("Pivot_ Branch and 2 more.pdf"));
    assert!(asked.borrow().iter().all(|s| !s.default_name.contains("Consolidated_ALCO")), "the fixed names are gone");
}
