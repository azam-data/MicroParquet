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

// ----------------------------------------------------------------------------- R1: cash-flow ladder

use crate::ui::layout_tests::Painted;
use crate::ui::render_tests::{assert_dialog_fits, inside, screen};
use crate::ui::render_tests_p4::{click_text, Rig, SCREENS};
use crate::ui::tests::parquet_source;
use micro_parquet::commands::CommandId;
use polars::prelude::{DataFrame, NamedFrom, Series};

fn dated(name: &str, days: &[&str]) -> Series {
    let day = |s: &str| (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date") - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch")).num_days() as i32;
    Series::new(name.into(), days.iter().map(|d| if d.is_empty() { None } else { Some(day(d)) }).collect::<Vec<Option<i32>>>()).cast(&polars::prelude::DataType::Date).expect("Date")
}

fn cf_title() -> String {
    format!("{}  IRRBB cash-flow ladder", theme::ph::CHART_BAR)
}

/// An "Assets" and a "Liabilities" tab (open in that order).
fn cf_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    let assets = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2"]),
        Series::new("Outstanding".into(), &[1_000_000.0, 250_000.0]),
        Series::new("Rate".into(), &[9.0, 9.0]),
        dated("Matdat", &["2031-04-30", "2026-10-31"]),
    ])
    .expect("assets");
    let liab = DataFrame::new(vec![
        Series::new("Account_No".into(), &["L1"]),
        Series::new("Outstanding".into(), &[300_000.0]),
        Series::new("Rate".into(), &[5.0]),
        dated("Matdat", &["2026-09-30"]),
    ])
    .expect("liabilities");
    for (title, df) in [("Assets", assets), ("Liabilities", liab)] {
        let src = parquet_source(&root, title, df);
        app.create_tab_from_source(title.into(), src, vec![], HashMap::new());
        settle(&mut app);
    }
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn overlaps(a: egui::Rect, b: egui::Rect) -> bool {
    a.intersects(b) && a.intersect(b).width() > 1.0 && a.intersect(b).height() > 1.0
}

#[test]
fn r1_the_cash_flow_dialog_fits_takes_roles_by_click_and_builds_the_ladder() {
    for (ppp, w, h) in SCREENS {
        let mut rig = cf_rig("p5r1a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::IrrbbLadder);
        assert_eq!(rig.app.active_tool, ActiveToolModal::IrrbbLadder, "{what}");
        let p: Painted = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &cf_title(), &what);
        // (the middle scrolls: what is below the fold is checked against its card, not the window)
        for t in ["Cutoff date (YYYY-MM-DD)", "Cash-flow mode", "Scenario", "Behaviour profile", "Tab", "Counts as", "Outstanding amount", "Maturity date", "Rate (%)", "Next repricing date", "Rate type", "Repayment type", "Build cash-flow ladder", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.cf.rects.clone();
        for name in ["Cutoff date", "Cash-flow mode", "Scenario", "Behaviour profile"] {
            let rc = g.settings.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{what}: '{name}' was drawn"));
            assert!(inside(rc, r.window) && rc.bottom() <= r.body.top() + 1.0 || inside(rc, r.header), "{what}: '{name}' {rc:?} is in the dialog's fixed header {:?}", r.header);
        }
        assert!(g.lines.add_button.left() >= r.window.left() && g.lines.add_button.right() <= r.window.right(), "{what}: the Add tab button is inside the dialog's width");
        assert_eq!(g.lines.cards.len(), 1, "{what}");
        for (i, name, c) in &g.lines.controls {
            assert!(inside(*c, g.lines.cards[*i]), "{what}: '{name}' {c:?} is inside its card {:?}", g.lines.cards[*i]);
        }
        // no two controls of a line overlap
        let ctrls: Vec<_> = g.lines.controls.iter().filter(|(i, _, _)| *i == 0).collect();
        for (a, (_, na, ra)) in ctrls.iter().enumerate() {
            for (_, nb, rb) in ctrls.iter().skip(a + 1) {
                assert!(!overlaps(*ra, *rb), "{what}: '{na}' {ra:?} overlaps '{nb}' {rb:?}");
            }
        }
        assert!(g.notes.left() >= r.window.left() && g.notes.right() <= r.window.right(), "{what}: the notes {:?} are inside the dialog's width {:?}", g.notes, r.window);
        for (name, b) in &g.buttons {
            assert!(inside(*b, r.footer), "{what}: '{name}' is inside the footer");
        }
        // the columns were guessed from their names
        let l = &rig.app.cf.lines[0];
        assert_eq!((l.principal_col.as_str(), l.maturity_col.as_str(), l.rate_col.as_str()), ("Outstanding", "Matdat", "Rate"), "{what}");
        rig.app.cf.cutoff = "2026-10-31".into();
        // the first line is Liabilities (the active tab): choose "Liability / outflow" by a real click
        let role = g.lines.controls.iter().find(|(i, n, _)| *i == 0 && n == "Counts as").map(|(_, _, r)| *r).expect("role combo");
        rig.click(role.center());
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Liability / outflow", role);
        assert_eq!(rig.app.cf.lines[0].role, crate::ui::gap_ladder::RoleChoice::Liability, "{what}: the choice was taken");
        // principal only: the mode is chosen by a real click too
        let mode = rig.app.cf.rects.settings.iter().find(|(n, _)| n == "Cash-flow mode").map(|(_, r)| *r).expect("mode combo");
        rig.click(mode.center());
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Principal only", mode);
        assert_eq!(rig.app.cf.mode, micro_parquet::cashflow::CashFlowMode::PrincipalOnly, "{what}");
        // add the assets tab as a second line
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Add tab", egui::Rect::EVERYTHING);
        assert_eq!(rig.app.cf.lines.len(), 2, "{what}");
        rig.app.cf.lines[1].tab = 0;
        rig.app.cf.lines[1].role = crate::ui::gap_ladder::RoleChoice::Asset;
        rig.draw(5);
        assert_dialog_fits(&rig.ctx, &cf_title(), &what);
        assert_eq!(rig.app.cf.rects.lines.cards.len(), 2, "{what}: two lines are drawn");
        let build = rig.app.cf.rects.buttons.iter().find(|(n, _)| n == "Build cash-flow ladder").map(|(_, r)| *r).expect("button");
        rig.click(build.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed: {}", rig.app.status_message);
        let tab = rig.app.tabs.last().expect("the ladder tab");
        assert_eq!(tab.title, "IRRBB cash-flow ladder", "{what}");
        let df = &tab.cached_slice;
        let assets: Vec<Option<f64>> = df.column("Assets").expect("col").f64().expect("f64").into_iter().collect();
        let liabs: Vec<Option<f64>> = df.column("Liabilities").expect("col").f64().expect("f64").into_iter().collect();
        assert_eq!(assets[10], Some(1_000_000.0), "{what}: the asset is in 4–5Y");
        assert_eq!(liabs[0], Some(-300_000.0), "{what}: the matured liability is in O/N");
        assert_eq!(assets[19], Some(250_000.0), "{what}: the matured asset is in the last row, outside the buckets");
        let bucket = df.column("Bucket").expect("col").str().expect("str").get(19).unwrap_or("").to_string();
        assert_eq!(bucket, "Matured assets, not in ladder (1)", "{what}");
    }
}

#[test]
fn r1_a_missing_column_is_said_in_the_dialog_and_blocks_the_button() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = cf_rig("p5r1b", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::IrrbbLadder);
    rig.app.cf.cutoff = "2026-10-31".into();
    rig.app.cf.lines[0].maturity_col.clear();
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("choose the maturity date column")), "the problem is said");
    assert!(rig.app.cf_ladder_source().is_err());
    rig.app.cf.lines[0].maturity_col = "Matdat".into();
    rig.app.cf.cutoff = "31/10/2026".into();
    rig.draw(3);
    assert!(rig.app.cf_ladder_source().err().unwrap_or_default().contains("YYYY-MM-DD"));
}

#[test]
fn r1_the_ladder_is_listed_under_treasury_and_alm_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::IrrbbLadder), Some("Treasury & ALM"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["irrbb cash-flow ladder", "irrbb cash flow", "prepayment cpr"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::IrrbbLadder, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = cf_rig("p5r1c", ppp, w, h);
        let p = rig.draw(4);
        let banking = p.boxes.iter().find(|(t, bx)| t.contains("Banking") && bx.bottom() < theme::TOP_BAR_H).map(|(_, b)| *b).expect("Banking");
        rig.click(banking.center());
        let p = rig.draw(3);
        let at = |t: &str| p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, b)| *b).unwrap_or_else(|| panic!("'{t}' in the menu"));
        assert!(at("Treasury & ALM").top() < at("IRRBB cash-flow ladder").top() && at("IRRBB cash-flow ladder").top() < at("Loans & credit").top(), "{}", rig.what);
    }
}
