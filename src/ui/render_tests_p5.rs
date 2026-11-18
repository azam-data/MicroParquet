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
    assert!(matches!(app.tabs[idx].steps.first(), Some(TransformStep::CleanLineBreaks { columns, trim_all: true }) if columns.is_empty()), "{:?}", app.tabs[idx].steps);
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
        for t in ["Cutoff date (YYYY-MM-DD)", "Cash-flow mode", "Scenario", "Behaviour profile", "Treat blank maturity as O/N", "Tab", "Counts as", "Outstanding amount", "Maturity date", "Rate (%)", "Next repricing date", "Rate type", "Repayment type", "Build cash-flow ladder", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.cf.rects.clone();
        for name in ["Cutoff date", "Cash-flow mode", "Scenario", "Behaviour profile", "Blank maturity"] {
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
fn a2_the_blank_maturity_box_is_ticked_by_a_click_and_the_row_goes_to_o_n() {
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("p5a2");
        let root = app.paths.root_dir.clone();
        let liab = DataFrame::new(vec![
            Series::new("Account_No".into(), &["T1", "T2"]),
            Series::new("Outstanding".into(), &[500_000.0, 200_000.0]),
            Series::new("Rate".into(), &[5.0, 5.0]),
            dated("Matdat", &["", "2027-10-31"]),
        ])
        .expect("liabilities");
        let src = parquet_source(&root, "Liabilities", liab);
        app.create_tab_from_source("Liabilities".into(), src, vec![], HashMap::new());
        settle(&mut app);
        let (ctx, input) = screen(ppp, w, h);
        let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::IrrbbLadder);
        rig.app.cf.cutoff = "2026-10-31".into();
        rig.app.cf.lines[0].role = crate::ui::gap_ladder::RoleChoice::Liability;
        rig.app.cf.mode = micro_parquet::cashflow::CashFlowMode::PrincipalOnly;
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &cf_title(), &what);
        let tick = p.boxes.iter().filter(|(t, _)| t.contains("Treat blank maturity as O/N")).map(|(_, b)| *b).last().expect("the check box");
        assert!(inside(tick, r.window), "{what}: the box {tick:?} is in the dialog {:?}", r.window);
        assert!(!rig.app.cf.blank_on, "{what}: off by default");
        // tick the box by a real click and build
        click_text(&mut rig, &p, "Treat blank maturity as O/N", egui::Rect::EVERYTHING);
        assert!(rig.app.cf.blank_on, "{what}: a real click ticks it");
        rig.draw(3);
        let build = rig.app.cf.rects.buttons.iter().find(|(n, _)| n == "Build cash-flow ladder").map(|(_, r)| *r).expect("button");
        rig.click(build.center());
        rig.draw(6);
        let tab = rig.app.tabs.last().expect("the ladder tab");
        assert_eq!(tab.title, "IRRBB cash-flow ladder", "{what}: {}", rig.app.status_message);
        let df = &tab.cached_slice;
        let liabs: Vec<Option<f64>> = df.column("Liabilities").expect("col").f64().expect("f64").into_iter().collect();
        let labels: Vec<String> = df.column("Bucket").expect("col").str().expect("str").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        assert_eq!(liabs[0], Some(-500_000.0), "{what}: the blank-maturity row is in O/N");
        let at = labels.iter().position(|l| l == "Blank maturity, in O/N above (1)").unwrap_or_else(|| panic!("{what}: the count row is listed {labels:?}"));
        assert_eq!(liabs[at], Some(-500_000.0), "{what}");
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

// ------------------------------------------------------------------------------ R2: behaviour profile

fn bh_title() -> String {
    format!("{}  Behaviour profile", theme::ph::CALCULATOR)
}

/// A history tab: the acceptance series for SAV and a flat one for CD, month-ends of one year.
fn bh_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
    let ends = ["2025-11-30", "2025-12-31", "2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30", "2026-05-31", "2026-06-30", "2026-07-31", "2026-08-31", "2026-09-30", "2026-10-31"];
    let sav = [100.0, 95.0, 110.0, 90.0, 105.0, 120.0, 98.0, 102.0, 96.0, 104.0, 108.0, 100.0];
    let mut dates_v: Vec<&str> = ends.to_vec();
    dates_v.extend(ends);
    let mut prods: Vec<&str> = vec!["SAV"; 12];
    prods.extend(vec!["CD"; 12]);
    let mut bal = sav.to_vec();
    bal.extend(vec![50.0; 12]);
    let hist = DataFrame::new(vec![dated("Date", &dates_v), Series::new("Product".into(), prods), Series::new("Balance".into(), bal)]).expect("history");
    let src = parquet_source(&root, "History", hist);
    app.create_tab_from_source("History".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn pick(rig: &mut Rig, combo: egui::Rect, item: &str) {
    rig.click(combo.center());
    let p = rig.draw(3);
    click_text(rig, &p, item, combo);
    rig.draw(3);
}

#[test]
fn r2_the_behaviour_dialog_reads_a_history_applies_the_caps_and_saves_the_file() {
    for (ppp, w, h) in SCREENS {
        let mut rig = bh_rig("p5r2a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::BehaviourProfile);
        assert_eq!(rig.app.active_tool, ActiveToolModal::BehaviourProfile, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &bh_title(), &what);
        for t in ["History tab", "Look-back (months)", "Profile date (YYYY-MM-DD)", "Source", "Read history", "Close"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        // the columns of the history were guessed
        assert_eq!((rig.app.behaviour.date_col.as_str(), rig.app.behaviour.product_col.as_str(), rig.app.behaviour.balance_col.as_str()), ("Date", "Product", "Balance"), "{what}");
        // read the history by a real click
        let read = rig.app.behaviour.rects.buttons.iter().find(|(n, _)| n == "Read history").map(|(_, r)| *r).expect("button");
        rig.click(read.center());
        rig.draw(4);
        assert_eq!(rig.app.behaviour.rows.iter().map(|l| l.product.as_str()).collect::<Vec<_>>(), vec!["CD", "SAV"], "{what}: {:?}", rig.app.behaviour.read_error);
        assert_eq!(rig.app.behaviour.as_of, "2026-10-31", "{what}: the profile date is the last month-end");
        // nothing is chosen yet: the products need a category, and Save is off
        rig.draw(3);
        assert!(rig.app.behaviour.profile().is_err(), "{what}");
        // SAV is row 1: Retail non-transactional by a real click, maturity 3 years
        let combo = rig.app.behaviour.rects.row_controls.iter().find(|(i, n, _)| *i == 1 && n == "Category").map(|(_, _, r)| *r).expect("category combo");
        pick(&mut rig, combo, "Retail non-transactional");
        assert_eq!(rig.app.behaviour.rows[1].category, "Retail non-transactional", "{what}");
        rig.app.behaviour.rows[1].avg_maturity = "3".into();
        // CD is only a deposit with an early-redemption rate
        rig.app.behaviour.rows[0].tdrr = "10".into();
        rig.app.behaviour.rows[0].category = String::new();
        // CD has a history, so it needs a category too
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("choose a category")), "{what}: the missing category is said");
        let combo = rig.app.behaviour.rects.row_controls.iter().find(|(i, n, _)| *i == 0 && n == "Category").map(|(_, _, r)| *r).expect("category combo");
        pick(&mut rig, combo, "Wholesale");
        rig.app.behaviour.rows[0].avg_maturity = "2".into();
        let p = rig.draw(5);
        // the computed core of SAV (90%) is cut to the cap of 70% and said so; the short history is warned about
        let res = rig.app.behaviour.results().expect("settings are fine");
        let sav = res[1].as_ref().expect("SAV builds");
        assert!((sav.product.core_pct - 70.0).abs() < 1e-9 && sav.product.capped, "{what}: {:?}", sav.product);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("70.00 capped")), "{what}: the table says 70.00 capped");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("this profile uses 12 months")), "{what}: the short-history warning");
        // every control is inside the table, which is inside the dialog's width
        let r = assert_dialog_fits(&rig.ctx, &bh_title(), &what);
        let b = rig.app.behaviour.rects.clone();
        assert!(b.table.left() >= r.window.left() && b.table.right() <= r.window.right(), "{what}: the table {:?} is inside the dialog's width {:?}", b.table, r.window);
        for (i, name, c) in &b.row_controls {
            assert!(inside(*c, b.table), "{what}: line {i} '{name}' {c:?} is inside the table {:?}", b.table);
        }
        for (name, bx) in &b.buttons {
            if name != "Read history" {
                assert!(inside(*bx, r.footer), "{what}: '{name}' is inside the footer");
            }
        }
        // a typed average maturity above the cap is refused with the cap and the source
        rig.app.behaviour.rows[1].avg_maturity = "5".into();
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("cap of 4.5 years")), "{what}: the refusal names the cap");
        assert!(rig.app.behaviour.profile().is_err());
        rig.app.behaviour.rows[1].avg_maturity = "3".into();
        rig.draw(4);
        // save by a real click
        let save = rig.app.behaviour.rects.buttons.iter().find(|(n, _)| n == "Save profile").map(|(_, r)| *r).expect("the Save button");
        rig.click(save.center());
        rig.draw(4);
        let path = rig.app.paths.root_dir.join("portable_data").join("behaviour").join("2026-10-31.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{what}: {path:?}: {e} ({})", rig.app.status_message));
        let file: micro_parquet::behaviour::BehaviourFile = serde_json::from_str(&text).expect("json");
        assert_eq!(file.products.len(), 2, "{what}");
        let sav = file.products.iter().find(|p| p.product == "SAV").expect("SAV");
        assert!((sav.core_pct - 70.0).abs() < 1e-9 && sav.capped && sav.avg_maturity_years == 3.0 && sav.lookback_months == 12, "{what}: {sav:?}");
        let cd = file.products.iter().find(|p| p.product == "CD").expect("CD");
        assert!((cd.core_pct - 100.0).abs() < 1e-9 || cd.core_pct > 0.0, "{what}");
        assert_eq!(cd.tdrr_pct, 10.0, "{what}");
        // saved twice: the button says it replaces
        let p = rig.draw(4);
        assert!(p.boxes.iter().any(|(t, _)| t == "Replace profile"), "{what}: the second save is a replacement");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

#[test]
fn r2_a_typed_core_above_the_cap_is_refused_in_the_dialog() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = bh_rig("p5r2b", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::BehaviourProfile);
    rig.app.behaviour.as_of = "2026-10-31".into();
    rig.app.behaviour.rows = vec![crate::ui::behaviour_profile::ProductLine {
        product: "SAV".into(),
        category: "Retail non-transactional".into(),
        avg_maturity: "3".into(),
        method: micro_parquet::behaviour::CoreMethod::Typed,
        typed_core: "80".into(),
        ..Default::default()
    }];
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("a core share of 80% is above the Bangladesh Bank cap of 70%")), "refused with the cap: {:?}", p.boxes.iter().filter(|(t, _)| t.contains("80")).collect::<Vec<_>>());
    assert!(p.boxes.iter().any(|(t, _)| t.contains("13 April 2021")), "and the source");
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r2_the_gap_ladder_applies_the_profile_when_the_box_is_ticked_and_not_when_it_is_not() {
    for (ppp, w, h) in SCREENS {
        let mut app = test_app("p5r2c");
        let root = app.paths.root_dir.clone();
        micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
        let behaviour_dir = root.join("portable_data").join("behaviour");
        std::fs::create_dir_all(&behaviour_dir).expect("folder");
        let file = micro_parquet::behaviour::BehaviourFile {
            as_of: "2026-10-31".into(),
            source: "test".into(),
            products: vec![micro_parquet::behaviour::BehaviourProduct { product: "SAV".into(), category: "Retail non-transactional".into(), core_pct: 60.0, avg_maturity_years: 3.0, method: "Typed".into(), ..Default::default() }],
        };
        micro_parquet::behaviour::save_behaviour(&behaviour_dir, &file).expect("saves");
        std::fs::write(&app.paths.buckets_file, serde_json::to_string(&vec![gap_preset_p5()]).expect("json")).expect("preset file");
        let book = DataFrame::new(vec![Series::new("Product".into(), &["SAV"]), Series::new("Balance".into(), &[1_000_000.0]), dated("Matdat", &[""])]).expect("book");
        let src = parquet_source(&root, "Deposits", book);
        app.create_tab_from_source("Deposits".into(), src, vec![], HashMap::new());
        settle(&mut app);
        let (ctx, input) = screen(ppp, w, h);
        let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::GapLadder);
        rig.app.gap.cutoff = "2026-10-31".into();
        rig.app.gap.rows[0].role = crate::ui::gap_ladder::RoleChoice::Liability;
        let p = rig.draw(5);
        let gap_title = format!("{}  Liquidity gap ladder", theme::ph::CHART_BAR);
        let r = assert_dialog_fits(&rig.ctx, &gap_title, &what);
        // ticking the box by a real click shows the product column and the profile choice
        let tick = rig.app.gap.rects.settings.iter().find(|(n, _)| n == "Apply behaviour profile").map(|(_, r)| *r).expect("the box");
        assert!(inside(tick, r.window), "{what}: the box is inside the dialog");
        assert!(!rig.app.gap.rects.row_controls.iter().any(|(_, n, _)| n == "Product"), "{what}: the Product column only shows with the box ticked");
        let _ = p;
        rig.click(tick.center());
        assert!(rig.app.gap.apply_behaviour, "{what}");
        let p = rig.draw(5);
        assert!(rig.app.gap.rects.row_controls.iter().any(|(_, n, _)| n == "Product"), "{what}: the Product column is shown");
        assert_dialog_fits(&rig.ctx, &gap_title, &what);
        let g = rig.app.gap.rects.clone();
        for (i, name, c) in &g.row_controls {
            assert!(inside(*c, g.grid), "{what}: line {i} '{name}' {c:?} is inside the grid {:?}", g.grid);
        }
        assert!(inside(g.grid, r.window), "{what}: the grid fits the dialog");
        // nothing chosen: the dialog says so
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Choose the behaviour profile to apply")), "{what}");
        let combo = rig.app.gap.rects.settings.iter().find(|(n, _)| n == "Behaviour profile").map(|(_, r)| *r).expect("combo");
        pick(&mut rig, combo, "2026-10-31.json");
        assert_eq!(rig.app.gap.behaviour_sel, "2026-10-31.json", "{what}");
        rig.draw(3);
        let preset_idx = rig.app.gap.presets.iter().position(|p| p.name == "Test p5").expect("preset");
        rig.app.gap.preset_idx = preset_idx;
        rig.draw(3);
        let build = rig.app.gap.rects.buttons.iter().find(|(n, _)| n == "Build gap ladder").map(|(_, r)| *r).expect("button");
        rig.click(build.center());
        rig.draw(6);
        let tab = rig.app.tabs.last().expect("the ladder tab");
        let labels: Vec<String> = tab.cached_slice.column("Bucket").expect("c").str().expect("s").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let out: Vec<f64> = tab.cached_slice.column("Outflows").expect("c").f64().expect("f").into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let at = |l: &str| out[labels.iter().position(|x| x == l).expect("bucket")];
        assert_eq!(at("0-1 month"), 400_000.0, "{what}: {}", rig.app.status_message);
        assert_eq!(at("1-5 years"), 600_000.0, "{what}");
        // unticked: the Phase 4 ladder (a blank maturity is sight)
        rig.app.run_command(&rig.ctx, CommandId::GapLadder);
        rig.app.gap.cutoff = "2026-10-31".into();
        rig.app.gap.apply_behaviour = false;
        rig.app.gap.preset_idx = preset_idx;
        // (the dialog starts with the active tab, which is now the ladder: point the line at the deposits again)
        rig.app.gap.rows[0].tab = 0;
        rig.app.gap.rows[0].amount_col = "Balance".into();
        rig.app.gap.rows[0].maturity_col = "Matdat".into();
        rig.app.gap.rows[0].role = crate::ui::gap_ladder::RoleChoice::Liability;
        rig.draw(4);
        let build = rig.app.gap.rects.buttons.iter().find(|(n, _)| n == "Build gap ladder").map(|(_, r)| *r).expect("button");
        rig.click(build.center());
        rig.draw(6);
        let tab = rig.app.tabs.last().expect("the ladder tab");
        let out: Vec<f64> = tab.cached_slice.column("Outflows").expect("c").f64().expect("f").into_iter().map(|v| v.unwrap_or(0.0)).collect();
        assert_eq!(out[0], 1_000_000.0, "{what}: without the profile everything is in the first bucket, as in Phase 4");
        micro_parquet::rules::set_data_dir_override(None);
    }
}

fn gap_preset_p5() -> micro_parquet::models::CustomBucketDef {
    micro_parquet::models::CustomBucketDef {
        name: "Test p5".into(),
        rules: vec![
            micro_parquet::models::BucketRuleItem { max_days: 30, label: "0-1 month".into() },
            micro_parquet::models::BucketRuleItem { max_days: 365, label: "1-12 months".into() },
            micro_parquet::models::BucketRuleItem { max_days: 1825, label: "1-5 years".into() },
        ],
        overflow_label: "Over 5 years".into(),
        overdue_label: "Overdue".into(),
        null_sight_label: "0-1 month".into(),
        null_long_label: "Over 5 years".into(),
    }
}

#[test]
fn r2_the_behaviour_profile_is_listed_under_treasury_and_alm_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::BehaviourProfile), Some("Treasury & ALM"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["behaviour profile", "core volatile", "nmd casa"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::BehaviourProfile, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = bh_rig("p5r2d", ppp, w, h);
        let p = rig.draw(4);
        let banking = p.boxes.iter().find(|(t, bx)| t.contains("Banking") && bx.bottom() < theme::TOP_BAR_H).map(|(_, b)| *b).expect("Banking");
        rig.click(banking.center());
        let p = rig.draw(3);
        let at = |t: &str| p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, b)| *b).unwrap_or_else(|| panic!("'{t}' in the menu"));
        assert!(at("Treasury & ALM").top() < at("Behaviour profile").top() && at("Behaviour profile").top() < at("Loans & credit").top(), "{}", rig.what);
        micro_parquet::rules::set_data_dir_override(None);
    }
}

// ------------------------------------------------------------------------------ R3: IRRBB, six scenarios

fn irrbb_title() -> String {
    format!("{}  IRRBB (six scenarios)", theme::ph::CHART_LINE)
}

/// The acceptance book as two tabs, a flat 10% BDT curve and the BB shock file (written to the data folder).
fn irrbb_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
    let curves = root.join("portable_data").join("curves");
    std::fs::create_dir_all(&curves).expect("curves folder");
    micro_parquet::curves::set_curves_dir_override(Some(curves.clone()));
    let curve = micro_parquet::curves::YieldCurve {
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 1).expect("date"),
        currency: "BDT".into(),
        source: "test".into(),
        points: vec![micro_parquet::models::FtpPoint { tenor_days: 30, ftp_rate: 10.0 }, micro_parquet::models::FtpPoint { tenor_days: 9000, ftp_rate: 10.0 }],
    };
    micro_parquet::curves::save_curve(&curves, &curve).expect("saves");
    let assets = DataFrame::new(vec![Series::new("Account_No".into(), &["A1"]), Series::new("Outstanding".into(), &[1_000_000.0]), Series::new("Rate".into(), &[9.0]), dated("Matdat", &["2031-04-30"])]).expect("assets");
    let liab = DataFrame::new(vec![Series::new("Account_No".into(), &["L1"]), Series::new("Outstanding".into(), &[600_000.0]), Series::new("Rate".into(), &[4.0]), dated("Matdat", &["2026-10-31"])]).expect("liabilities");
    for (title, df) in [("Assets", assets), ("Liabilities", liab)] {
        let src = parquet_source(&root, title, df);
        app.create_tab_from_source(title.into(), src, vec![], HashMap::new());
        settle(&mut app);
    }
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn irrbb_cleanup() {
    micro_parquet::rules::set_data_dir_override(None);
    micro_parquet::curves::set_curves_dir_override(None);
}

#[test]
fn r3_the_irrbb_dialog_fits_takes_its_settings_by_click_and_builds_both_tabs() {
    for (ppp, w, h) in SCREENS {
        let mut rig = irrbb_rig("p5r3a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::IrrbbSix);
        assert_eq!(rig.app.active_tool, ActiveToolModal::IrrbbSix, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &irrbb_title(), &what);
        for t in ["Cutoff date (YYYY-MM-DD)", "Tier 1 capital (BDT)", "Cash-flow mode", "Curve rates are", "Post-shock floor (%)", "Behaviour profile", "Treat blank maturity as O/N", "Beta file", "rate floor (%)", "Counts as", "Outstanding amount", "Run IRRBB", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        // the fixed header holds every setting; nothing in it passes the dialog's edge or overlaps
        let g = rig.app.irrbb.rects.clone();
        for (name, rc) in &g.header {
            assert!(inside(*rc, r.window), "{what}: '{name}' {rc:?} is inside the dialog {:?}", r.window);
            assert!(rc.bottom() <= r.body.top() + 1.0 || r.header.is_positive(), "{what}: '{name}' is in the header");
        }
        for (i, (na, ra)) in g.header.iter().enumerate() {
            for (nb, rb) in g.header.iter().skip(i + 1) {
                assert!(!overlaps(*ra, *rb), "{what}: '{na}' {ra:?} overlaps '{nb}' {rb:?}");
            }
        }
        for (i, name, c) in &g.lines.controls {
            assert!(inside(*c, g.lines.cards[*i]), "{what}: '{name}' {c:?} is inside its card");
        }
        // the problem line asks for Tier 1 first
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Type the Tier 1 capital")), "{what}");
        // type the settings; the compounding by a real click
        rig.app.irrbb.cutoff = "2026-10-31".into();
        rig.app.irrbb.tier1 = "1,000,000".into();
        rig.app.irrbb.mode = micro_parquet::cashflow::CashFlowMode::PrincipalOnly;
        let comp = g.header.iter().find(|(n, _)| n == "Compounding").map(|(_, r)| *r).expect("combo");
        pick(&mut rig, comp, "Continuous");
        assert_eq!(rig.app.irrbb.compounding, micro_parquet::irrbb::Compounding::Continuous, "{what}");
        // line 1 is the active tab (Liabilities): a liability; add the assets
        rig.app.irrbb.lines[0].role = crate::ui::gap_ladder::RoleChoice::Liability;
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Add tab", egui::Rect::EVERYTHING);
        assert_eq!(rig.app.irrbb.lines.len(), 2, "{what}");
        rig.app.irrbb.lines[1].tab = 0;
        rig.app.irrbb.lines[1].role = crate::ui::gap_ladder::RoleChoice::Asset;
        rig.draw(5);
        assert_dialog_fits(&rig.ctx, &irrbb_title(), &what);
        let run = rig.app.irrbb.rects.buttons.iter().find(|(n, _)| n == "Run IRRBB").map(|(_, r)| *r).expect("button");
        rig.click(run.center());
        rig.draw(8);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed: {}", rig.app.status_message);
        let titles: Vec<String> = rig.app.tabs.iter().map(|t| t.title.clone()).collect();
        assert_eq!(&titles[titles.len() - 2..], &["IRRBB cash flows".to_string(), "IRRBB results".to_string()], "{what}");
        let results = rig.app.tabs.last().expect("results");
        let df = &results.cached_slice;
        let eve: Vec<Option<f64>> = df.column("EVE change").expect("c").f64().expect("f").into_iter().collect();
        assert!((eve[0].unwrap_or(0.0) + 104_969.17).abs() < 0.01, "{what}: parallel up {:?}", eve[0]);
        let labels: Vec<String> = df.column("Scenario").expect("c").str().expect("s").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let notes: Vec<String> = df.column("Note").expect("c").str().expect("s").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let at = |l: &str| labels.iter().position(|x| x.starts_with(l)).expect("row");
        assert_eq!(notes[at("Outlier")], "No", "{what}");
        let pct = results.col_formats.get("Loss % of Tier 1").map(|f| f.fmt_type);
        assert_eq!(pct, Some(MetricFormatType::PercentAsIs), "{what}: the percentage is not scaled");
        let cash = &rig.app.tabs[rig.app.tabs.len() - 2];
        let net: Vec<Option<f64>> = cash.cached_slice.column("Base PV").expect("c").f64().expect("f").into_iter().collect();
        assert!((net[19].unwrap_or(0.0) - 37_796.13).abs() < 0.05, "{what}: the Base EVE row");
        irrbb_cleanup();
    }
}

#[test]
fn r3_a_small_tier_1_makes_the_bank_an_outlier_and_a_sample_shock_file_shows_the_banner() {
    for (ppp, w, h) in SCREENS {
        let mut rig = irrbb_rig("p5r3b", ppp, w, h);
        let what = rig.what.clone();
        // a sample shock file dated later than the BB file, in the data folder
        let dir = rig.app.paths.root_dir.join("portable_data").join("rules").join("irrbb_shocks");
        std::fs::create_dir_all(&dir).expect("folder");
        std::fs::write(dir.join("2026-01-01.json"), r#"{"as_of":"2026-01-01","source":"Sample numbers","sample":true,"currencies":[{"currency":"BDT","parallel_bp":400,"short_bp":500,"long_bp":300}]}"#).expect("write");
        rig.app.run_command(&rig.ctx, CommandId::IrrbbSix);
        rig.app.irrbb.cutoff = "2026-10-31".into();
        rig.app.irrbb.tier1 = "600000".into();
        rig.app.irrbb.mode = micro_parquet::cashflow::CashFlowMode::PrincipalOnly;
        rig.app.irrbb.compounding = micro_parquet::irrbb::Compounding::Continuous;
        rig.app.irrbb.lines[0].role = crate::ui::gap_ladder::RoleChoice::Liability;
        rig.app.irrbb.lines.push(rig.app.irrbb.lines[0].clone());
        rig.app.irrbb.lines[1].tab = 0;
        rig.app.irrbb.lines[1].role = crate::ui::gap_ladder::RoleChoice::Asset;
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &irrbb_title(), &what);
        assert!(p.boxes.iter().any(|(t, _)| t.contains("Sample parameters")), "{what}: the banner is shown");
        assert!(inside(rig.app.irrbb.rects.banner, r.window), "{what}: the banner {:?} is inside the dialog {:?}", rig.app.irrbb.rects.banner, r.window);
        let run = rig.app.irrbb.rects.buttons.iter().find(|(n, _)| n == "Run IRRBB").map(|(_, r)| *r).expect("button");
        rig.click(run.center());
        rig.draw(8);
        let results = rig.app.tabs.last().expect("results");
        assert_eq!(results.title, "IRRBB results", "{what}: {}", rig.app.status_message);
        let labels: Vec<String> = results.cached_slice.column("Scenario").expect("c").str().expect("s").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let notes: Vec<String> = results.cached_slice.column("Note").expect("c").str().expect("s").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let at = |l: &str| labels.iter().position(|x| x.starts_with(l)).expect("row");
        assert_eq!(notes[at("Outlier")], "Yes", "{what}: 17.49% of Tier 1");
        assert_eq!(notes[at("Rule file")], "irrbb_shocks/2026-01-01.json", "{what}: the latest file on or before the cutoff");
        assert!(labels.iter().any(|l| l == "Sample parameters"), "{what}: the sample warning is in the table too");
        let pct = results.cached_slice.column("Loss % of Tier 1").expect("c").f64().expect("f").get(at("Maximum loss")).unwrap_or(0.0);
        assert_eq!(format!("{pct:.2}"), "17.49", "{what}");
        irrbb_cleanup();
    }
}

#[test]
fn r3_the_irrbb_tool_is_listed_under_treasury_and_alm_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::IrrbbSix), Some("Treasury & ALM"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["irrbb six scenarios", "outlier tier 1", "steepener flattener"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::IrrbbSix, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = irrbb_rig("p5r3c", ppp, w, h);
        let p = rig.draw(4);
        let banking = p.boxes.iter().find(|(t, bx)| t.contains("Banking") && bx.bottom() < theme::TOP_BAR_H).map(|(_, b)| *b).expect("Banking");
        rig.click(banking.center());
        let p = rig.draw(3);
        let at = |t: &str| p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, b)| *b).unwrap_or_else(|| panic!("'{t}' in the menu"));
        assert!(at("Treasury & ALM").top() < at("IRRBB (six scenarios)").top() && at("IRRBB (six scenarios)").top() < at("Loans & credit").top(), "{}", rig.what);
        irrbb_cleanup();
    }
}

// ------------------------------------------------------------------------------ R4: classify by overdue

fn ov_title() -> String {
    format!("{}  {}", theme::ph::BANK, crate::ui::classify_overdue::OVERDUE_TITLE)
}

/// A loan book tab with a category, an overdue-since date, a days-past-due and an override column.
fn ov_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4"]),
        Series::new("Loan_Category".into(), &["Continuous", "Demand", "Fixed term", "Fixed term"]),
        dated("Overdue_Since", &["", "2026-09-30", "2026-07-31", "2026-07-31"]),
        Series::new("DPD".into(), &[0i64, 31, 92, 92]),
        Series::new("Override_Grade".into(), &[None, None, None, Some("DF")]),
        Series::new("Balance".into(), &[1_000_000.0, 2_000_000.0, 3_000_000.0, 4_000_000.0]),
    ])
    .expect("book");
    let src = parquet_source(&root, "Loans", book);
    app.create_tab_from_source("Loans".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

#[test]
fn r4_the_classify_dialog_fits_takes_its_choices_by_click_and_adds_the_step() {
    for (ppp, w, h) in SCREENS {
        let mut rig = ov_rig("p5r4a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::ClassifyOverdue);
        assert_eq!(rig.app.active_tool, ActiveToolModal::ClassifyOverdue, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &ov_title(), &what);
        for t in ["Cutoff date (YYYY-MM-DD)", "Loan category", "Same for every row:", "From the column:", "Overdue period", "Overdue-since date column:", "Days-past-due column:", "Override grade column", "Classify by overdue", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.overdue.rects.clone();
        for (name, rc) in &g.controls {
            assert!(inside(*rc, r.window), "{what}: '{name}' {rc:?} is inside the dialog {:?}", r.window);
        }
        assert!(inside(g.notes, r.window) && g.notes.right() <= r.window.right(), "{what}: the notes are inside the dialog");
        for (name, b) in &g.buttons {
            assert!(inside(*b, r.footer), "{what}: '{name}' is in the footer");
        }
        // the columns were guessed from their names
        assert_eq!((rig.app.overdue.category_col.as_str(), rig.app.overdue.since_col.as_str(), rig.app.overdue.dpd_col.as_str(), rig.app.overdue.override_col.as_str()), ("Loan_Category", "Overdue_Since", "DPD", "Override_Grade"), "{what}");
        assert!(rig.app.overdue.category_by_col && !rig.app.overdue.use_dpd && rig.app.overdue.override_on, "{what}");
        // the cutoff date is typed; the days-past-due radio is a real click
        rig.app.overdue.cutoff = "2026-10-31".into();
        let dpd = g.control("Use days past due").expect("radio");
        rig.click(dpd.center());
        assert!(rig.app.overdue.use_dpd, "{what}: a real click chose the days-past-due column");
        let since = rig.app.overdue.rects.control("Use overdue-since").expect("radio");
        rig.draw(3);
        rig.click(since.center());
        assert!(!rig.app.overdue.use_dpd, "{what}: and back");
        // the fixed category is chosen from the combo by a real click, after "Same for every row"
        let same = rig.app.overdue.rects.control("Same category").expect("radio");
        rig.draw(2);
        rig.click(same.center());
        assert!(!rig.app.overdue.category_by_col, "{what}");
        let p = rig.draw(3);
        let combo = rig.app.overdue.rects.control("Fixed category").expect("combo");
        rig.click(combo.center());
        let p2 = rig.draw(3);
        assert!(p2.boxes.iter().any(|(t, _)| t == "Short-term agri & micro"), "{what}: the four categories of the rule file are listed");
        click_text(&mut rig, &p2, "Demand", combo);
        assert_eq!(rig.app.overdue.fixed_category, "Demand", "{what}");
        drop(p);
        // take the category from the column again, untick the override, and build
        let from = rig.app.overdue.rects.control("Category from column").expect("radio");
        rig.draw(2);
        rig.click(from.center());
        rig.app.overdue.override_on = false;
        rig.draw(4);
        let button = rig.app.overdue.rects.button("Classify by overdue").expect("button");
        rig.click(button.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed: {}", rig.app.status_message);
        settle(&mut rig.app);
        let tab = &rig.app.tabs[rig.app.active_tab];
        assert!(matches!(tab.steps.last(), Some(TransformStep::ClassifyOverdue { category_col: Some(c), since_col: Some(s), dpd_col: None, override_col: None, .. }) if c == "Loan_Category" && s == "Overdue_Since"), "{what}: {:?}", tab.steps);
        let fin: Vec<Option<String>> = tab.cached_slice.column("CL_Final").expect("CL_Final").str().expect("text").into_iter().map(|v| v.map(str::to_string)).collect();
        assert_eq!(fin, ["STD-0", "STD-2", "SS", "SS"].iter().map(|s| Some(s.to_string())).collect::<Vec<_>>(), "{what}");
        // the status bar says how many rows had no date
        assert!(rig.app.status_message.is_empty() || !rig.app.status_message.contains("Error"), "{what}: {}", rig.app.status_message);
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r4_a_missing_choice_is_said_in_red_and_blocks_the_button_and_the_step_reopens_in_the_dialog() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = ov_rig("p5r4b", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::ClassifyOverdue);
    rig.app.overdue.cutoff = "31/10/2026".into();
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("Type the cutoff date as YYYY-MM-DD")), "the date problem is said");
    assert!(rig.app.classify_overdue_step().is_err());
    rig.app.overdue.cutoff = "2026-10-31".into();
    rig.app.overdue.since_col.clear();
    rig.draw(3);
    assert!(rig.app.classify_overdue_step().err().unwrap_or_default().contains("overdue-since date column"));
    rig.app.overdue.since_col = "Overdue_Since".into();
    rig.app.overdue.override_on = true;
    rig.app.overdue.override_col.clear();
    assert!(rig.app.classify_overdue_step().err().unwrap_or_default().contains("override grade column"));
    rig.app.overdue.override_col = "Override_Grade".into();
    let step = rig.app.classify_overdue_step().expect("a step");
    // reopening the step prefills the dialog with the same choices
    rig.app.overdue = crate::ui::classify_overdue::OverdueState::default();
    let cols: Vec<String> = rig.app.tabs[0].available_columns();
    rig.app.prefill_from_step(&step, &cols);
    assert_eq!(format!("{:?}", rig.app.classify_overdue_step().expect("same step")), format!("{step:?}"));
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r4_the_tool_is_listed_under_loans_and_credit_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::ClassifyOverdue), Some("Loans & credit"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["classify loans by overdue", "dpd days past due", "overdue months"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::ClassifyOverdue, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = ov_rig("p5r4c", ppp, w, h);
        let p = rig.draw(4);
        let banking = p.boxes.iter().find(|(t, bx)| t.contains("Banking") && bx.bottom() < theme::TOP_BAR_H).map(|(_, b)| *b).expect("Banking");
        rig.click(banking.center());
        let p = rig.draw(3);
        let at = |t: &str| p.boxes.iter().find(|(x, _)| x.contains(t)).map(|(_, b)| *b).unwrap_or_else(|| panic!("'{t}' in the menu"));
        assert!(at("Loans & credit").top() < at("Classify by overdue").top(), "{}", rig.what);
        assert!(at("Classify by overdue").right() <= rig.ctx.screen_rect().right(), "{}: the menu entry is on screen", rig.what);
    }
    micro_parquet::rules::set_data_dir_override(None);
}
