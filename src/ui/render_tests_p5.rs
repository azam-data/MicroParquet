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
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Treasury & ALM", "IRRBB cash-flow ladder");
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
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Treasury & ALM", "Behaviour profile");
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
        std::fs::write(dir.join("2026-01-01.json"), r#"{"as_of":"2026-01-01","source":"Sample numbers","sample":true,"decay_x":4,"outlier_pct":15,"currencies":[{"currency":"BDT","parallel_bp":400,"short_bp":500,"long_bp":300}]}"#).expect("write");
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
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Treasury & ALM", "IRRBB (six scenarios)");
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
        for t in ["Cutoff date (YYYY-MM-DD)", "Loan category", "Same for every row:", "From the column:", "Overdue period", "Overdue-since date column:", "Days-past-due column:", "Override grade column", "Overdue amount column", "Classify by overdue", "Cancel"] {
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
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Loans & credit", "Classify by overdue");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------ R5: CL-1 summary

fn cl1_title() -> String {
    format!("{}  {}", theme::ph::TABLE, crate::ui::cl1_summary::CL1_TITLE)
}

/// The six-loan book of the R5 test, as a tab; the template and the maps are written under the data folder.
fn cl1_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3", "A4", "A5", "A6"]),
        Series::new("Loan_Category".into(), &["Continuous", "CONTINUOUS", "Demand", "Fixed term", "Fixed term", "Short-term agri & micro"]),
        Series::new("CL_Final".into(), &["STD-0", "STD-2", "SMA", "SS", "B/L", "DF"]),
        Series::new("Balance".into(), &[1000.0, 2000.0, 500.0, 300.0, 100.0, 400.0]),
        Series::new("Base_For_Provision_BDT".into(), &[1000.0, 2000.0, 500.0, 250.0, 100.0, 300.0]),
        Series::new("Required_Provision_BDT".into(), &[10.0, 20.0, 25.0, 50.0, 100.0, 150.0]),
    ])
    .expect("book");
    let src = parquet_source(&root, "Loans", book);
    app.create_tab_from_source("Loans".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

#[test]
fn r5_the_cl1_dialog_fits_shows_the_sample_banner_and_builds_the_summary_tab() {
    for (ppp, w, h) in SCREENS {
        let mut rig = cl1_rig("p5r5a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Cl1Summary);
        assert_eq!(rig.app.active_tool, ActiveToolModal::Cl1Summary, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &cl1_title(), &what);
        for t in ["Sample parameters", "Loan category", "Same for every row:", "From the column:", "Grade column", "Measure", "Outstanding", "Base for provision", "Provision required", "Build CL-1 summary", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.cl1.rects.clone();
        assert!(inside(g.banner, r.header) || inside(g.banner, r.window), "{what}: the sample banner is in the header: {:?}", g.banner);
        for (name, rc) in &g.controls {
            assert!(inside(*rc, r.window), "{what}: '{name}' {rc:?} is inside the dialog {:?}", r.window);
        }
        assert!(g.notes.right() <= r.window.right() && g.notes.left() >= r.window.left(), "{what}: the notes are inside the dialog's width");
        for (name, b) in &g.buttons {
            assert!(inside(*b, r.footer), "{what}: '{name}' is in the footer");
        }
        // the columns were guessed
        let s = &rig.app.cl1;
        assert_eq!((s.category_col.as_str(), s.grade_col.as_str()), ("Loan_Category", "CL_Final"), "{what}");
        assert_eq!(s.measure_cols.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(), vec!["Balance", "Base_For_Provision_BDT", "Required_Provision_BDT"], "{what}");
        let build = g.button("Build CL-1 summary").expect("button");
        rig.click(build.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed: {}", rig.app.status_message);
        settle(&mut rig.app);
        let tab = rig.app.tabs.last().expect("the summary tab");
        assert_eq!(tab.title, "CL-1 summary", "{what}");
        let df = &tab.cached_slice;
        assert_eq!(df.height(), 5, "{what}");
        let v = |c: &str, r: usize| df.column(c).expect("column").f64().expect("f64").get(r).unwrap_or(f64::NAN);
        assert_eq!(v("Total | Outstanding", 4), 4300.0, "{what}");
        assert_eq!(v("Total | Provision required", 4), 355.0, "{what}");
        assert_eq!(v("STD | Outstanding", 0), 3000.0, "{what}");

        // the Excel export of that tab (the file dialog is not part of the call)
        let out = rig.app.paths.root_dir.join("cl1_out.xlsx");
        rig.app.export_cl1_excel_to(out.clone());
        settle(&mut rig.app);
        assert!(out.exists(), "{what}: the workbook was written: {}", rig.app.status_message);
        // on any other tab the command says what to do and opens no file dialog
        rig.app.active_tab = 0;
        rig.app.export_cl1_excel();
        assert!(rig.app.status_message.contains("not a CL-1 summary"), "{what}: {}", rig.app.status_message);
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r5_a_missing_choice_is_said_in_red_and_a_template_error_is_shown() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = cl1_rig("p5r5b", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::Cl1Summary);
    rig.app.cl1.grade_col.clear();
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("Choose the grade column")), "the grade problem is said");
    assert!(rig.app.cl1_source().is_err());
    rig.app.cl1.grade_col = "CL_Final".into();
    rig.app.cl1.measure_cols[1].1.clear();
    assert!(rig.app.cl1_source().err().unwrap_or_default().contains("Choose the column of 'Base for provision'"));
    rig.app.cl1.measure_cols[1].1 = "Base_For_Provision_BDT".into();
    assert!(rig.app.cl1_source().is_ok());
    // a template that names an unknown grade is shown in the dialog with its name
    let f = rig.app.paths.root_dir.join("portable_data").join("report_templates").join("cl1.json");
    let bad = std::fs::read_to_string(&f).expect("written").replace("\"SMA\"]", "\"SMA-9\"]");
    std::fs::write(&f, bad).expect("edit");
    rig.app.run_command(&rig.ctx, CommandId::Cl1Summary);
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("'SMA-9'")), "the template error names the grade");
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r5_the_tool_is_listed_under_loans_and_credit_and_its_export_under_export() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::Cl1Summary), Some("Loans & credit"));
    assert!(in_group(CommandGroup::Export).any(|c| c.id == CommandId::ExportCl1), "the Excel export of the CL-1 summary is in the Export menu");
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["cl-1 summary", "cl1 return"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::Cl1Summary, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = cl1_rig("p5r5c", ppp, w, h);
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Loans & credit", "CL-1 summary");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------ R6: LCR / NSFR

fn liq_title() -> String {
    format!("{}  {}", theme::ph::CHART_BAR, crate::ui::liquidity::LIQ_TITLE)
}

/// A GL tab whose codes fit the shipped sample map, plus one code (999900) that no rule covers.
fn liq_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
    let gl = DataFrame::new(vec![
        Series::new("GL_Code".into(), &["101000", "103000", "104000", "201000", "202000", "203000", "115000", "300100", "999900"]),
        Series::new("Closing_Balance".into(), &[1000.0, 400.0, 100.0, -10_000.0, -10_000.0, -2_500.0, 3_600.0, -500.0, 5.0]),
        Series::new("Residual_Maturity".into(), &[None, None, None, None, None, None, Some(24.0), None, None]),
    ])
    .expect("gl");
    let src = parquet_source(&root, "GL", gl);
    app.create_tab_from_source("GL".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

#[test]
fn r6_the_dialog_fits_stops_on_an_unmapped_code_and_builds_both_tabs_with_another_map() {
    for (ppp, w, h) in SCREENS {
        let mut rig = liq_rig("p5r6a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Liquidity);
        assert_eq!(rig.app.active_tool, ActiveToolModal::Liquidity, "{what}");
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &liq_title(), &what);
        for t in ["Sample parameters", "Liquidity map", "GL code column", "Balance column", "Balance sign", "Residual maturity column (NSFR)", "Calculate LCR and NSFR", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.liq.rects.clone();
        assert!(g.banner.is_positive() && inside(g.banner, r.window), "{what}: the sample banner is drawn: {:?}", g.banner);
        for (name, rc) in &g.controls {
            assert!(inside(*rc, r.window), "{what}: '{name}' {rc:?} is inside the dialog {:?}", r.window);
        }
        for (name, b) in &g.buttons {
            assert!(inside(*b, r.footer), "{what}: '{name}' is in the footer");
        }
        // the columns were guessed from their names, and the sample map is chosen
        let s = &rig.app.liq;
        assert_eq!((s.gl_col.as_str(), s.balance_col.as_str(), s.maturity_col.as_str(), s.map_sel.as_str()), ("GL_Code", "Closing_Balance", "Residual_Maturity", "sample_basel.json"), "{what}");
        // Calculate: 999900 is in no rule of the map, so the run stops and says so; the tabs are not made
        let tabs_before = rig.app.tabs.len();
        let calc = g.button("Calculate LCR and NSFR").expect("button");
        rig.click(calc.center());
        let p = rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::Liquidity, "{what}: the dialog stays open");
        assert_eq!(rig.app.tabs.len(), tabs_before, "{what}: no tab was made");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("1 GL code(s) are not in the liquidity map") && t.contains("999900")), "{what}: the stop names the code");
        let export = rig.app.liq.rects.control("Export unmapped").expect("the Export unmapped button is shown");
        assert!(inside(export, r.window), "{what}: Export unmapped {export:?} is in the dialog");
        let out = rig.app.paths.root_dir.join("unmapped.csv");
        rig.app.export_unmapped_to(out.clone());
        assert_eq!(std::fs::read_to_string(&out).expect("written"), "GL code,Balance\n999900,5\n", "{what}");
        // a map of the bank's own that covers the code: written next to the sample, chosen by a real click
        let sample = std::fs::read_to_string(rig.app.paths.root_dir.join("portable_data").join("liquidity_maps").join("sample_basel.json")).expect("the sample was written");
        let mut j: serde_json::Value = serde_json::from_str(&sample).expect("json");
        j["gl_map"].as_array_mut().expect("rules").push(serde_json::json!({"match": "prefix", "value": "9999", "line": "RSF_CASH"}));
        j["sample"] = serde_json::json!(false);
        std::fs::write(rig.app.paths.root_dir.join("portable_data").join("liquidity_maps").join("bank.json"), j.to_string()).expect("write the bank's map");
        rig.app.open_liquidity();
        rig.draw(4);
        let combo = rig.app.liq.rects.control("Liquidity map").expect("map combo");
        pick(&mut rig, combo, "bank.json");
        assert_eq!(rig.app.liq.map_sel, "bank.json", "{what}");
        let p = rig.draw(5);
        assert!(!p.boxes.iter().any(|(t, _)| t.contains("Sample parameters")), "{what}: no banner for a map that is not a sample");
        let calc = rig.app.liq.rects.button("Calculate LCR and NSFR").expect("button");
        rig.click(calc.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed: {} {}", rig.app.liq.error, rig.app.status_message);
        settle(&mut rig.app);
        assert_eq!(rig.app.tabs.len(), tabs_before + 2, "{what}");
        let lines = &rig.app.tabs[tabs_before];
        let summary = &rig.app.tabs[tabs_before + 1];
        assert_eq!((lines.title.as_str(), summary.title.as_str()), ("LCR/NSFR lines", "LCR/NSFR summary"), "{what}");
        let items: Vec<String> = summary.cached_slice.column("Item").expect("Item").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let val = |i: &str| summary.cached_slice.column("Value").expect("Value").f64().expect("f64").get(items.iter().position(|x| x == i).unwrap_or(0)).unwrap_or(f64::NAN);
        assert!((val("HQLA after caps") - 1390.0).abs() < 0.005, "{what}: {}", val("HQLA after caps"));
        assert!((val("LCR") - 198.5714).abs() < 0.001, "{what}: {}", val("LCR"));
        assert!((val("NSFR") - 20250.0 / 3060.0 * 100.0).abs() < 0.001, "{what}: {}", val("NSFR"));
        assert!(lines.cached_slice.height() >= 10, "{what}");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r6_a_missing_choice_is_said_in_red_and_blocks_the_button() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = liq_rig("p5r6b", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::Liquidity);
    rig.app.liq.gl_col.clear();
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("Choose the GL code column")), "the problem is said");
    assert!(rig.app.liquidity_inputs().is_err());
    rig.app.liq.gl_col = "GL_Code".into();
    rig.app.liq.balance_col = "Nope".into();
    assert!(rig.app.liquidity_inputs().err().unwrap_or_default().contains("Choose the balance column"));
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r6_the_tool_is_listed_under_treasury_and_alm_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::Liquidity), Some("Treasury & ALM"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["lcr nsfr", "liquidity coverage", "net stable funding"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::Liquidity, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = liq_rig("p5r6c", ppp, w, h);
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Treasury & ALM", "LCR / NSFR from GL balances");
    }
    micro_parquet::rules::set_data_dir_override(None);
}


#[test]
fn the_banking_menu_fits_the_window_with_every_entry_on_screen_at_both_scales() {
    use micro_parquet::commands::*;
    // every Banking command is under one of the two headings
    for c in in_group(CommandGroup::Banking) {
        assert!(banking_section(c.id).is_some(), "{:?} has no heading in the Banking menu", c.id);
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = cl1_rig("p5menu", ppp, w, h);
        // every Banking command is listed under its heading of the Analyse menu, on screen
        for c in in_group(CommandGroup::Banking) {
            let title = c.title.trim_end_matches('\u{2026}');
            let section = banking_section(c.id).expect("heading");
            crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, section, title);
            rig.draw(2);
            for _ in 0..2 {
                rig.step(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: Some(egui::Key::Escape), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
            }
        }
    }
    micro_parquet::rules::set_data_dir_override(None);
}


// ------------------------------------------------------------------------------ R7: IFRS 9 ECL

fn ifrs_title() -> String {
    format!("{}  {}", theme::ph::BANK, crate::ui::ifrs9::IFRS9_TITLE)
}

/// Three loans (performing, 45 and 95 days past due) of the sample "Corporate" segment, two years from the maturity.
fn ifrs_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    micro_parquet::rules::set_data_dir_override(Some(root.join("portable_data")));
    let book = DataFrame::new(vec![
        Series::new("Account_No".into(), &["A1", "A2", "A3"]),
        Series::new("Segment".into(), &["Corporate", "Corporate", "Corporate"]),
        Series::new("Drawn".into(), &[1_000_000.0, 1_000_000.0, 1_000_000.0]),
        Series::new("Undrawn".into(), &[0.0, 0.0, 0.0]),
        Series::new("EIR".into(), &[10.0, 10.0, 10.0]),
        dated("Matdat", &["2028-10-31", "2028-10-31", "2028-10-31"]),
        Series::new("DPD".into(), &[0i64, 45, 95]),
        Series::new("CL_Final".into(), &["STD-0", "STD-2", "DF"]),
        Series::new("Required_Provision_BDT".into(), &[10_000.0, 10_000.0, 500_000.0]),
    ])
    .expect("book");
    let src = parquet_source(&root, "Loans", book);
    app.create_tab_from_source("Loans".into(), src, vec![], HashMap::new());
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

#[test]
fn r7_the_dialog_fits_adds_the_step_and_opens_the_summary_tab() {
    for (ppp, w, h) in SCREENS {
        let mut rig = ifrs_rig("p5r7a", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::IfrsEcl);
        assert_eq!(rig.app.active_tool, ActiveToolModal::IfrsEcl, "{what}");
        rig.app.ecl.cutoff = "2026-10-31".into();
        let p = rig.draw(6);
        let r = assert_dialog_fits(&rig.ctx, &ifrs_title(), &what);
        for t in ["Sample parameters", "Cutoff date (YYYY-MM-DD)", "Segment", "Same for every row:", "From the column:", "Drawn amount", "Undrawn amount", "Effective interest rate (%)", "Maturity date", "Days past due", "Classification grade", "Forborne (Y/N)", "SICR not assessed (Y/N)", "Rating at origination", "Current rating", "Previous stage", "Days performing", "Regulatory provision (summary)", "Classified grades (SS, DF, B/L) are Stage 3", "Calculate IFRS 9 ECL", "Calculate ECL and open the summary", "Cancel"] {
            assert!(p.boxes.iter().any(|(x, bx)| x.contains(t) && inside(*bx, r.window)), "{what}: '{t}' is in the dialog {:?}", r.window);
        }
        let g = rig.app.ecl.rects.clone();
        assert!(g.banner.is_positive() && inside(g.banner, r.window), "{what}: the sample banner is drawn");
        for (name, rc) in &g.controls {
            assert!(inside(*rc, r.window), "{what}: '{name}' {rc:?} is inside the dialog {:?}", r.window);
        }
        for (name, b) in &g.buttons {
            assert!(inside(*b, r.footer), "{what}: '{name}' is in the footer");
        }
        assert!(g.notes.left() >= r.window.left() && g.notes.right() <= r.window.right(), "{what}: the notes are inside the dialog");
        // the columns were guessed from their names, and the classified option is on (the rule file's default)
        let s = &rig.app.ecl;
        assert_eq!((s.drawn_col.as_str(), s.eir_col.as_str(), s.maturity_col.as_str(), s.dpd_col.as_str(), s.grade_col.as_str(), s.provision_col.as_str(), s.segment_col.as_str()), ("Drawn", "EIR", "Matdat", "DPD", "CL_Final", "Required_Provision_BDT", "Segment"), "{what}");
        assert!(s.classified_stage3 && s.segment_by_col, "{what}");
        // the "classified" box is ticked off by a real click
        let tick = g.control("Classified are Stage 3").expect("check box");
        rig.click(tick.center());
        assert!(!rig.app.ecl.classified_stage3, "{what}: a real click unticks it");
        rig.click(tick.center());
        assert!(rig.app.ecl.classified_stage3, "{what}: and ticks it again");
        // the first button adds the step to the tab
        let button = rig.app.ecl.rects.button("Calculate IFRS 9 ECL").expect("button");
        rig.draw(3);
        rig.click(button.center());
        rig.draw(6);
        assert_eq!(rig.app.active_tool, ActiveToolModal::None, "{what}: the dialog closed: {}", rig.app.status_message);
        settle(&mut rig.app);
        let tab = &rig.app.tabs[rig.app.active_tab];
        assert!(matches!(tab.steps.last(), Some(TransformStep::IfrsEcl { classified_stage3: true, .. })), "{what}: {:?}", tab.steps);
        let col = |c: &str| tab.cached_slice.column(c).unwrap_or_else(|_| panic!("{what}: column {c}")).clone();
        let stage: Vec<Option<i64>> = col("Stage").cast(&polars::prelude::DataType::Int64).expect("cast").i64().expect("i64").into_iter().collect();
        assert_eq!(stage, vec![Some(1), Some(2), Some(3)], "{what}");
        let ecl: Vec<Option<f64>> = col("ECL").f64().expect("f64").into_iter().collect();
        // the sample parameters: cumulative PD 2% and 5%, LGD 45%, scenarios 50/25/25 at x1 / x0.8 / x1.5 (1.075 times the single scenario)
        assert!((ecl[0].unwrap_or(f64::NAN) - 8795.45).abs() < 0.005 && (ecl[1].unwrap_or(f64::NAN) - 20789.26).abs() < 0.005 && (ecl[2].unwrap_or(f64::NAN) - 450_000.0).abs() < 0.005, "{what}: {ecl:?}");
        // the status bar note (a sample has three scenarios: no warning)
        // the second button: the step and the summary tab
        let mut rig2 = ifrs_rig("p5r7b", ppp, w, h);
        rig2.app.run_command(&rig2.ctx, CommandId::IfrsEcl);
        rig2.app.ecl.cutoff = "2026-10-31".into();
        rig2.draw(6);
        let tabs_before = rig2.app.tabs.len();
        let b2 = rig2.app.ecl.rects.button("Calculate ECL and open the summary").expect("button");
        rig2.click(b2.center());
        rig2.draw(6);
        settle(&mut rig2.app);
        assert_eq!(rig2.app.tabs.len(), tabs_before + 1, "{what}: the summary tab was added");
        assert!(matches!(rig2.app.tabs[0].steps.last(), Some(TransformStep::IfrsEcl { .. })), "{what}: the step is in the loan tab, not the summary: {:?}", rig2.app.tabs[0].steps);
        let sum = rig2.app.tabs.last().expect("summary");
        assert_eq!(sum.title, "IFRS 9 summary", "{what}");
        let group: Vec<String> = sum.cached_slice.column("Group").expect("Group").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let section: Vec<String> = sum.cached_slice.column("Section").expect("Section").str().expect("text").into_iter().map(|v| v.unwrap_or("").to_string()).collect();
        let row = (0..group.len()).find(|i| group[*i] == "Total" && section[*i] == "Grade group").expect("total row");
        let f = |c: &str| sum.cached_slice.column(c).expect("column").f64().expect("f64").get(row).unwrap_or(f64::NAN);
        assert!((f("ECL") - (8795.45 + 20789.26 + 450_000.0)).abs() < 0.01, "{what}: {}", f("ECL"));
        assert!((f("Regulatory provision") - 520_000.0).abs() < 0.005, "{what}");
        assert!((f("Higher of") - 520_000.0).abs() < 0.005, "{what}: the provision is the higher of the two");
        assert!((f("Shortfall to Loan Loss Reserve") - (520_000.0 - 479_584.71)).abs() < 0.01, "{what}: {}", f("Shortfall to Loan Loss Reserve"));
    }
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r7_a_missing_choice_is_said_in_red_and_the_step_reopens_prefilled() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = ifrs_rig("p5r7c", ppp, w, h);
    rig.app.run_command(&rig.ctx, CommandId::IfrsEcl);
    rig.app.ecl.cutoff = "31/10/2026".into();
    let p = rig.draw(6);
    assert!(p.boxes.iter().any(|(t, _)| t.contains("Type the cutoff date as YYYY-MM-DD")), "the date problem is said");
    rig.app.ecl.cutoff = "2026-10-31".into();
    rig.app.ecl.eir_col.clear();
    assert!(rig.app.ifrs9_step().err().unwrap_or_default().contains("Choose the effective interest rate column"));
    rig.app.ecl.eir_col = "EIR".into();
    rig.app.ecl.prev_col = "DPD".into();
    assert!(rig.app.ifrs9_step().err().unwrap_or_default().contains("both the previous stage and the days performing"));
    rig.app.ecl.prev_col.clear();
    rig.app.ecl.rating_orig_col = "DPD".into();
    assert!(rig.app.ifrs9_step().err().unwrap_or_default().contains("both rating columns"));
    rig.app.ecl.rating_orig_col.clear();
    let step = rig.app.ifrs9_step().expect("a step");
    // the second button needs the provision column
    rig.app.ecl.provision_col.clear();
    rig.draw(4);
    let b2 = rig.app.ecl.rects.button("Calculate ECL and open the summary").expect("button");
    let tabs = rig.app.tabs.len();
    rig.click(b2.center());
    rig.draw(3);
    assert_eq!(rig.app.tabs.len(), tabs, "the disabled button does nothing");
    // reopen the step: the same choices
    rig.app.ecl = crate::ui::ifrs9::EclState::default();
    let cols = rig.app.tabs[0].available_columns();
    rig.app.prefill_from_step(&step, &cols);
    assert_eq!(format!("{:?}", rig.app.ifrs9_step().expect("same step")), format!("{step:?}"));
    micro_parquet::rules::set_data_dir_override(None);
}

#[test]
fn r7_the_tool_is_listed_under_loans_and_credit_and_found_by_the_search() {
    use micro_parquet::commands::*;
    assert_eq!(banking_section(CommandId::IfrsEcl), Some("Loans & credit"));
    let entries: Vec<Entry> = COMMANDS.iter().map(|c| Entry { label: c.title, words: c.words, recent: None }).collect();
    for q in ["ifrs 9 ecl", "expected credit loss", "stage 2 sicr"] {
        let ranked = rank(q, &entries);
        assert_eq!(COMMANDS[ranked[0]].id, CommandId::IfrsEcl, "{q}");
    }
    for (ppp, w, h) in SCREENS {
        let mut rig = ifrs_rig("p5r7d", ppp, w, h);
        crate::ui::render_tests_p4::assert_analyse_lists(&mut rig, "Loans & credit", "IFRS 9 ECL");
    }
    micro_parquet::rules::set_data_dir_override(None);
}

// ------------------------------------------------------------------------------ C1: the Steps panel buttons

fn steps_rig(name: &str, ppp: f32, w: f32, h: f32) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    let df = DataFrame::new(vec![
        Series::new("Actnum".into(), &["A1", "A2"]),
        Series::new("Balance".into(), &[1.0, 2.0]),
        Series::new("Rate".into(), &[9.0, 8.0]),
    ])
    .expect("book");
    let src = parquet_source(&root, "Steps", df);
    // three steps that can be moved and copied in any order
    let steps = vec![
        TransformStep::CleanLineBreaks { columns: vec!["Actnum".into()], trim_all: false },
        TransformStep::CleanLineBreaks { columns: vec![], trim_all: false },
        TransformStep::CleanLineBreaks { columns: vec![], trim_all: true },
    ];
    open_tab(&mut app, src, steps);
    let (ctx, input) = screen(ppp, w, h);
    Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") }
}

fn step_labels(rig: &Rig) -> Vec<String> {
    rig.app.tabs[rig.app.active_tab].steps.iter().map(|s| s.short_label()).collect()
}

/// Clicks the card of step `i` and returns the buttons drawn for it.
fn select_step(rig: &mut Rig, i: usize) -> crate::ui::steps::StepsReport {
    rig.draw(3);
    let rep = crate::ui::steps::steps_report(&rig.ctx).expect("steps report");
    let card = rep.cards[i];
    rig.click(egui::pos2(card.left() + 20.0, card.top() + 8.0));
    assert_eq!(rig.app.selected_step, Some(i), "{}: a click on the card of step {i} selects it", rig.what);
    rig.draw(3);
    crate::ui::steps::steps_report(&rig.ctx).expect("steps report")
}

fn press_step_button(rig: &mut Rig, name: &str) {
    let rep = crate::ui::steps::steps_report(&rig.ctx).expect("steps report");
    let b = rep.named_buttons.iter().find(|(n, _)| n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("{}: no '{name}' button", rig.what));
    rig.click(b.center());
}

#[test]
fn c1_every_step_button_does_its_job_when_its_real_rect_is_clicked_and_all_six_fit_in_one_row() {
    for (ppp, w, h) in SCREENS {
        let mut rig = steps_rig("p5c1", ppp, w, h);
        let what = rig.what.clone();
        let rep = select_step(&mut rig, 1);
        // six buttons, one row, inside the card and the panel, delete at the right end, no overlaps
        let names: Vec<&str> = rep.named_buttons.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["edit", "up", "down", "hide", "duplicate", "delete"], "{what}");
        let top = rep.named_buttons[0].1.top();
        for (n, r) in &rep.named_buttons {
            assert!((r.top() - top).abs() < 1.0 && r.height() <= theme::STEP_BTN_H + 1.0, "{what}: '{n}' {r:?} is in the same row");
            assert!(r.right() <= rep.cards[1].right() + 0.5 && r.right() <= rep.inner.right() + 0.5 && r.left() >= rep.cards[1].left(), "{what}: '{n}' {r:?} is inside the card {:?}", rep.cards[1]);
        }
        for (i, (na, a)) in rep.named_buttons.iter().enumerate() {
            for (nb, b) in rep.named_buttons.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "{what}: '{na}' and '{nb}' overlap");
            }
        }
        let delete = rep.named_buttons[5].1;
        assert!(rep.named_buttons.iter().all(|(_, r)| r.right() <= delete.right() + 0.5), "{what}: delete is at the right end");
        assert!(rep.cards[1].right() - delete.right() < theme::MARGIN_STEP_ROW.right + 6.0, "{what}: and reaches the card's right margin");

        // move up: step 1 goes first
        press_step_button(&mut rig, "up");
        assert_eq!(step_labels(&rig)[0], "Clean line breaks (all text columns)", "{what}: {:?}", step_labels(&rig));
        // move down: it goes back to second place
        let _ = select_step(&mut rig, 0);
        press_step_button(&mut rig, "down");
        assert_eq!(step_labels(&rig)[1], "Clean line breaks (all text columns)", "{what}: {:?}", step_labels(&rig));
        // hide: the step is turned off, and on again
        let _ = select_step(&mut rig, 1);
        assert!(!rig.app.tabs[rig.app.active_tab].steps[1].is_skipped());
        press_step_button(&mut rig, "hide");
        assert!(rig.app.tabs[rig.app.active_tab].steps[1].is_skipped(), "{what}: the step is off");
        let _ = select_step(&mut rig, 1);
        press_step_button(&mut rig, "hide");
        assert!(!rig.app.tabs[rig.app.active_tab].steps[1].is_skipped(), "{what}: the step is on again");
        // duplicate: four steps, the copy next to the original
        let _ = select_step(&mut rig, 2);
        press_step_button(&mut rig, "duplicate");
        let labels = step_labels(&rig);
        assert_eq!(labels.len(), 4, "{what}: {labels:?}");
        assert_eq!(labels[2], labels[3], "{what}: {labels:?}");
        // delete asks first, then removes
        let _ = select_step(&mut rig, 3);
        press_step_button(&mut rig, "delete");
        assert_eq!(rig.app.confirm_delete_step, Some(3), "{what}: asks first");
        assert_eq!(step_labels(&rig).len(), 4, "{what}: nothing is removed yet");
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Delete step?", egui::Rect::EVERYTHING);
        assert_eq!(step_labels(&rig).len(), 3, "{what}: the step is removed: {:?}", step_labels(&rig));
        // and "No" keeps it
        let _ = select_step(&mut rig, 2);
        press_step_button(&mut rig, "delete");
        let p = rig.draw(3);
        click_text(&mut rig, &p, "No", egui::Rect::EVERYTHING);
        assert_eq!(step_labels(&rig).len(), 3, "{what}: No keeps it");
        // edit: the step's dialog opens, editing that step
        let _ = select_step(&mut rig, 0);
        press_step_button(&mut rig, "edit");
        assert_eq!(rig.app.active_tool, ActiveToolModal::CleanLineBreaks, "{what}: the editor of the step opened");
        assert!(rig.app.editing.is_some(), "{what}");
    }
}

#[test]
fn c1_the_buttons_of_the_first_and_last_step_say_why_up_and_down_are_off_and_a_click_on_a_button_does_not_select_another_step() {
    let (ppp, w, h) = SCREENS[0];
    let mut rig = steps_rig("p5c1b", ppp, w, h);
    select_step(&mut rig, 0);
    let before = step_labels(&rig);
    press_step_button(&mut rig, "up");
    assert_eq!(step_labels(&rig), before, "the first step cannot move up");
    select_step(&mut rig, 2);
    press_step_button(&mut rig, "down");
    assert_eq!(step_labels(&rig), before, "the last step cannot move down");
    // a click on the selected step's button leaves it selected (it is not taken by the card)
    assert_eq!(rig.app.selected_step, Some(2));
}

// ------------------------------------------------------------------------------ C1 audit: other rows with buttons

/// The painted icon `glyph`s below the tab strip, top to bottom.
fn glyphs(p: &Painted, glyph: &str) -> Vec<egui::Rect> {
    let mut v: Vec<egui::Rect> = p.boxes.iter().filter(|(t, b)| t == glyph && b.top() > theme::TOP_BAR_H + theme::TAB_STRIP_H).map(|(_, b)| *b).collect();
    v.sort_by(|a, b| a.top().partial_cmp(&b.top()).unwrap_or(std::cmp::Ordering::Equal).then(a.left().partial_cmp(&b.left()).unwrap_or(std::cmp::Ordering::Equal)));
    v
}

#[test]
fn c1_audit_the_sort_level_buttons_work_by_a_real_click() {
    for (ppp, w, h) in SCREENS {
        let mut rig = steps_rig("p5c1s", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Sort);
        rig.app.sort.levels = vec![SortLevel { column: "Actnum".into(), descending: false, ..Default::default() }, SortLevel { column: "Balance".into(), descending: false, ..Default::default() }];
        let cols = |rig: &Rig| rig.app.sort.levels.iter().map(|l| l.column.clone()).collect::<Vec<_>>();
        let p = rig.draw(4);
        // move the first level down
        let down = glyphs(&p, theme::ph::ARROW_DOWN)[0];
        rig.click(down.center());
        assert_eq!(cols(&rig), vec!["Balance", "Actnum"], "{what}: move down");
        // move the second level up
        let p = rig.draw(3);
        let up = glyphs(&p, theme::ph::ARROW_UP)[1];
        rig.click(up.center());
        assert_eq!(cols(&rig), vec!["Actnum", "Balance"], "{what}: move up");
        // remove the first level
        let p = rig.draw(3);
        let x = glyphs(&p, theme::ph::X)[0];
        rig.click(x.center());
        assert_eq!(cols(&rig), vec!["Balance"], "{what}: remove");
        // add a level
        let p = rig.draw(3);
        click_text(&mut rig, &p, "Add level", egui::Rect::EVERYTHING);
        assert_eq!(rig.app.sort.levels.len(), 2, "{what}: add");
    }
}

#[test]
fn c1_audit_the_filter_condition_buttons_work_by_a_real_click() {
    for (ppp, w, h) in SCREENS {
        let mut rig = steps_rig("p5c1f", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::Filter);
        rig.draw(4);
        let n0 = rig.app.filter.conditions.len();
        let p = rig.draw(2);
        click_text(&mut rig, &p, "Add condition", egui::Rect::EVERYTHING);
        assert_eq!(rig.app.filter.conditions.len(), n0 + 1, "{what}: add a condition");
        let p = rig.draw(3);
        let crosses = glyphs(&p, theme::ph::X);
        assert!(!crosses.is_empty(), "{what}: the conditions have a remove button");
        rig.click(crosses[0].center());
        assert_eq!(rig.app.filter.conditions.len(), n0, "{what}: remove a condition");
    }
}

#[test]
fn c1_audit_the_behaviour_profile_row_remove_button_and_the_curve_point_remove_button_work_by_a_real_click() {
    for (ppp, w, h) in SCREENS {
        // behaviour profile: add a product, remove it with its cross
        let mut rig = bh_rig("p5c1b", ppp, w, h);
        let what = rig.what.clone();
        rig.app.run_command(&rig.ctx, CommandId::BehaviourProfile);
        rig.draw(4);
        let n0 = rig.app.behaviour.rows.len();
        let p = rig.draw(2);
        let add = p.boxes.iter().filter(|(t, b)| t.ends_with("Add product") && b.width() < 200.0).map(|(_, b)| *b).next().expect("the Add product button");
        rig.click(add.center());
        assert_eq!(rig.app.behaviour.rows.len(), n0 + 1, "{what}: add a product");
        let p = rig.draw(3);
        let table = rig.app.behaviour.rects.table;
        let x: Vec<egui::Rect> = glyphs(&p, theme::ph::X).into_iter().filter(|b| inside(*b, table)).collect();
        assert_eq!(x.len(), n0 + 1, "{what}: one cross per product row");
        rig.click(x[x.len() - 1].center());
        assert_eq!(rig.app.behaviour.rows.len(), n0, "{what}: remove a product");
        micro_parquet::rules::set_data_dir_override(None);

        // yield curves: a click on a list row opens the curve; a point is removed with its cross
        let mut rig = crate::ui::render_tests_p4::curves_rig("p5c1c", ppp, w, h);
        rig.app.run_command(&rig.ctx, CommandId::YieldCurves);
        rig.draw(5);
        let row = rig.app.curves.rects.list_rows.iter().find(|(d, _)| d == "2026-01-01").map(|(_, r)| *r).expect("row");
        rig.click(row.center());
        assert_eq!(rig.app.curves.editor.date, "2026-01-01", "{what}: a click on the row opens the curve");
        let n = rig.app.curves.editor.points.len();
        let p = rig.draw(3);
        let editor = rig.app.curves.rects.editor;
        let x: Vec<egui::Rect> = glyphs(&p, theme::ph::X).into_iter().filter(|b| inside(*b, editor)).collect();
        assert_eq!(x.len(), n, "{what}: one cross per point");
        rig.click(x[0].center());
        assert_eq!(rig.app.curves.editor.points.len(), n - 1, "{what}: remove a point");
        micro_parquet::curves::set_curves_dir_override(None);
    }
}


// ------------------------------------------------------------------------------ C2: the Column inspector

fn insp_rig(name: &str, ppp: f32, w: f32, h: f32, df: DataFrame, column: &str) -> Rig {
    let mut app = test_app(name);
    let root = app.paths.root_dir.clone();
    let src = parquet_source(&root, "Insp", df);
    open_tab(&mut app, src, vec![]);
    app.select_column(column);
    settle(&mut app);
    let (ctx, input) = screen(ppp, w, h);
    let mut rig = Rig { app, ctx, input, what: format!("ppp {ppp} {w}x{h}") };
    rig.draw(4);
    rig
}

/// Scrolls the right panel down by turning the mouse wheel over it.
fn scroll_panel_down(rig: &mut Rig, points: f32) -> Painted {
    let pos = egui::pos2(rig.ctx.screen_rect().right() - theme::RIGHT_PANEL_W / 2.0, rig.ctx.screen_rect().center().y);
    rig.step(vec![egui::Event::PointerMoved(pos)]);
    rig.step(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -points), modifiers: egui::Modifiers::NONE }]);
    // the scroll is animated: let it finish
    let mut last = rig.draw(1);
    for _ in 0..8 {
        rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 0.5);
        last = rig.draw(1);
    }
    last
}

fn hover(rig: &mut Rig, pos: egui::Pos2) -> Painted {
    rig.step(vec![egui::Event::PointerMoved(pos)]);
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
    rig.step(vec![egui::Event::PointerMoved(pos)]);
    rig.input.time = Some(rig.input.time.unwrap_or(0.0) + 1.0);
    rig.step(vec![egui::Event::PointerMoved(pos)])
}

#[test]
fn c2_stat_cards_never_cut_a_value_a_card_that_does_not_fit_takes_the_whole_row_and_every_card_has_a_tooltip() {
    for (ppp, w, h) in SCREENS {
        for (sum, want) in [(258_251_800_000.0f64, "25,825.18 Cr"), (12_345_678_900_000.0f64, "1,234,567.89 Cr"), (1_234_567_890_100_000.0f64, "123,456,789.01 Cr")] {
            let df = DataFrame::new(vec![Series::new("Balance".into(), &[sum])]).expect("df");
            let mut rig = insp_rig("p5c2a", ppp, w, h, df, "Balance");
            let what = format!("{} sum {sum}", rig.what);
            let p = rig.draw(3);
            let rep = crate::ui::steps::inspector_report(&rig.ctx).unwrap_or_else(|| panic!("{what}: no inspector report"));
            let names: Vec<&str> = rep.tiles.iter().map(|(k, _, _)| k.as_str()).collect();
            assert_eq!(names, vec!["Rows", "Blanks", "Parse failures", "Sum", "Min", "Max", "Mean", "Distinct"], "{what}");
            // one row: the sum, min, max and mean are all the value
            let (_, sum_text, sum_rect) = rep.tiles.iter().find(|(k, _, _)| k == "Sum").cloned().expect("Sum");
            let (_, min_text, _) = rep.tiles.iter().find(|(k, _, _)| k == "Min").cloned().expect("Min");
            assert!(min_text.ends_with(" Cr") && sum_text.ends_with(" Cr"), "{what}: {sum_text} / {min_text}");
            assert_eq!((sum_text.as_str(), min_text.as_str()), (want, want), "{what}");
            for (k, v, r) in rep.tiles.iter().filter(|(_, _, r)| r.bottom() < rep.inner.bottom() - 4.0) {
                // the whole value is painted, with no ellipsis, inside its card and the panel
                let b = p.boxes.iter().filter(|(t, bx)| t == v && inside(*bx, *r)).map(|(_, b)| *b).next().unwrap_or_else(|| panic!("{what}: the value '{v}' of '{k}' is painted whole inside its card {r:?}"));
                assert!(!v.contains('\u{2026}') && !p.boxes.iter().any(|(t, _)| t.contains('\u{2026}') && t.starts_with(v.trim_end_matches('\u{2026}')) && t != v), "{what}: '{k}' is cut");
                assert!(r.right() <= rep.inner.right() + 0.5 && r.left() >= rep.inner.left() - 0.5, "{what}: card of '{k}' {r:?} is inside the panel {:?}", rep.inner);
                assert!(b.right() <= r.right() - theme::MARGIN_TILE.right + 0.5, "{what}: '{v}' {b:?} fits its card {r:?}");
            }
            // a value that does not fit a half card has the whole row; one that fits shares it
            let half = theme::TILE_W + 2.0 * theme::MARGIN_TILE.left;
            for (k, v, r) in rep.tiles.iter().filter(|(_, _, r)| r.bottom() < rep.inner.bottom() - 4.0) {
                let vb = p.boxes.iter().find(|(t, bx)| t == v && inside(*bx, *r)).map(|(_, b)| *b).expect("value");
                if vb.width() > theme::TILE_W {
                    assert!(r.width() > half + 20.0, "{what}: the card of '{k}' ({v}) does not fit a half card and spans the panel: {r:?}");
                } else {
                    assert!((r.width() - half).abs() < 1.0, "{what}: the card of '{k}' is a half card: {r:?}");
                }
            }
            if want.len() > 16 {
                assert!(rep.tiles.iter().any(|(k, _, r)| k == "Sum" && r.width() > half + 20.0), "{what}: a value of {} characters has a card of its own row", want.len());
            }
            // two short cards share a row
            let rows_r = rep.tiles.iter().find(|(k, _, _)| k == "Rows").map(|(_, _, r)| *r).expect("Rows");
            let blanks_r = rep.tiles.iter().find(|(k, _, _)| k == "Blanks").map(|(_, _, r)| *r).expect("Blanks");
            assert!((rows_r.top() - blanks_r.top()).abs() < 1.0 && rows_r.right() < blanks_r.left(), "{what}: Rows and Blanks share a row");
            // hovering a card gives its full value
            let pt = sum_rect.center();
            let tip = hover(&mut rig, pt);
            assert!(tip.boxes.iter().any(|(t, _)| t == &format!("Sum: {sum_text}")), "{what}: the tooltip of the Sum card gives the full value: {:?}", tip.boxes.iter().filter(|(t, _)| t.contains("Sum")).collect::<Vec<_>>());
        }
    }
}

#[test]
fn c2_a_skewed_column_shows_its_bars_between_the_percentiles_the_outliers_a_log_toggle_and_a_tooltip_per_bar() {
    for (ppp, w, h) in SCREENS {
        let mut v: Vec<f64> = (1..=100).map(|i| i as f64).filter(|i| *i != 50.0).collect();
        v.push(1e9);
        let df = DataFrame::new(vec![Series::new("Rate_Value".into(), v)]).expect("df");
        let mut rig = insp_rig("p5c2b", ppp, w, h, df, "Rate_Value");
        let what = rig.what.clone();
        rig.draw(3);
        let p = scroll_panel_down(&mut rig, 2000.0);
        let rep = crate::ui::steps::inspector_report(&rig.ctx).expect("report");
        assert_eq!(rep.bars.len(), 10, "{what}");
        assert!(p.boxes.iter().any(|(t, _)| t.contains("1 value above range")), "{what}: the outlier is said: {:?}", p.boxes.iter().filter(|(t, _)| t.contains("range")).collect::<Vec<_>>());
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("10 bars from 2") && t.contains("1st to the 99th percentile")), "{what}: the caption gives the bar count and the range: {:?}", p.boxes.iter().filter(|(t, _)| t.contains("bars from")).collect::<Vec<_>>());
        for b in &rep.bars {
            assert!(b.right() <= rep.inner.right() + 0.5 && b.left() >= rep.inner.left() - 0.5, "{what}: bar slot {b:?} in the panel {:?}", rep.inner);
        }
        // more than one bar has values: the painted bars (accent-coloured rectangles) are several
        let accent = theme::tokens(false).accent.gamma_multiply(theme::HISTOGRAM_BAR_ALPHA);
        let painted_bars = p.rects.iter().filter(|(r, f)| *f == accent && rep.bars.iter().any(|b| b.contains(r.center()))).count();
        assert!(painted_bars > 1, "{what}: more than one non-empty bar: {painted_bars}");
        // a tooltip per bar: range and count
        let tip = hover(&mut rig, rep.bars[3].center());
        assert!(tip.boxes.iter().any(|(t, _)| t.contains(" to ") && t.contains(" value")), "{what}: the bar says its range and count: {:?}", tip.boxes.iter().filter(|(t, _)| t.contains("value")).collect::<Vec<_>>());
        // the Log scale toggle: a real click
        let toggle = p.boxes.iter().find(|(t, _)| t == "Log scale").map(|(_, b)| *b).expect("the Log scale check box");
        assert!(!rig.app.inspector_log);
        rig.click(toggle.center());
        assert!(rig.app.inspector_log, "{what}: a click turns the log scale on");
        let rep2 = crate::ui::steps::inspector_report(&rig.ctx).expect("report");
        assert_eq!(rep2.bars.len(), 10, "{what}: still ten bars");
        rig.click(toggle.center());
        assert!(!rig.app.inspector_log, "{what}: and off");
    }
}

#[test]
fn c2_a_text_column_lists_its_top_ten_and_a_date_column_its_months() {
    for (ppp, w, h) in SCREENS {
        let mut v: Vec<String> = Vec::new();
        for i in 0..13 {
            for _ in 0..(15 - i) {
                v.push(format!("Branch{i:02}"));
            }
        }
        let df = DataFrame::new(vec![Series::new("Brancd".into(), v)]).expect("df");
        let mut rig = insp_rig("p5c2c", ppp, w, h, df, "Brancd");
        let what = rig.what.clone();
        rig.draw(3);
        let p = scroll_panel_down(&mut rig, 140.0);
        assert!(p.boxes.iter().any(|(t, _)| t == "Most frequent (top 10)"), "{what}");
        let listed = (0..13).filter(|i| p.boxes.iter().any(|(t, _)| t == &format!("Branch{i:02}"))).count();
        assert_eq!(listed, 10, "{what}: ten values are listed");
        assert!(p.boxes.iter().any(|(t, _)| t == "15") && p.boxes.iter().any(|(t, _)| t == "6"), "{what}: with their counts");

        let d = dated("Opndat", &["2026-01-05", "2026-01-31", "2026-03-01", "2025-12-31", "2026-03-15", "2026-03-20"]);
        let df = DataFrame::new(vec![d]).expect("df");
        let mut rig = insp_rig("p5c2d", ppp, w, h, df, "Opndat");
        rig.draw(3);
        let p = scroll_panel_down(&mut rig, 2000.0);
        let rep = crate::ui::steps::inspector_report(&rig.ctx).expect("report");
        assert_eq!(rep.bars.len(), 3, "{what}: a bar for each month with values");
        assert!(p.boxes.iter().any(|(t, _)| t.starts_with("3 months from 2025-12 to 2026-03")), "{what}: the caption names the months");
        let tip = hover(&mut rig, rep.bars[2].center());
        assert!(tip.boxes.iter().any(|(t, _)| t == "2026-03: 3 values"), "{what}: {:?}", tip.boxes.iter().filter(|(t, _)| t.contains("2026")).collect::<Vec<_>>());
    }
}


