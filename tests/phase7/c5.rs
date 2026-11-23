//! C5: the banking chart templates. Each template is run on the example of its tool's guide (the real
//! builders where they are cheap to call, the tool's table as the guide prints it otherwise): the form
//! it gives, the exact finding title, and the chart drawn by the three renderers.
//!
//! The finding titles follow the reference images in `docs/design/charts/` (one decimal and a true minus
//! sign: "−10.5% of Tier 1"; "Breach: −16.7% vs −10% limit"); the spec's printed examples had two decimals.

use crate::p7_c1::{all_sizes, check, lay};
use micro_parquet::banking::*;
use micro_parquet::chart::{ChartKind, ChartSpec};
use micro_parquet::charts::calc::compute;
use micro_parquet::charts::data::{waterfall_from_data, ChartData};
use micro_parquet::charts::echarts::chart_html;
use micro_parquet::charts::layout::{chart_title, LayoutOpts, MarkKind, Prim, Role};
use micro_parquet::charts::samples::*;
use micro_parquet::charts::templates::{detect, fits, prepare, yield_curves, Ctx, Template};
use micro_parquet::charts::tokens as t;
use micro_parquet::models::*;
use polars::prelude::*;

const EN: &str = "\u{2013}";

fn opts() -> LayoutOpts {
    LayoutOpts { dark: false, divisor: 1.0, decimals: 2, unit: "BDT Crore".into(), bare: false, colours: None }
}


// ------------------------------------------------------------------------------------ the inputs

/// The Phase 4 V3 example: assets 100 and 200, liabilities 150 and 50, a limit of −10% on the first bucket.
pub(crate) fn v3_ladder(limit: Option<f64>) -> DataFrame {
    let amounts = |amt: &[f64], mat: &[&str]| DataFrame::new(vec![Series::new("Balance".into(), amt), dates("Matdat", mat)]).unwrap();
    let a = amounts(&[100.0, 200.0], &["2026-11-15", "2026-12-31"]);
    let l = amounts(&[150.0, 50.0], &["2026-11-20", "2026-12-15"]);
    let spec = |role| GapInputSpec { role, amount_col: "Balance".into(), maturity_col: "Matdat".into() };
    let preset = crate::v3::two_bucket_preset();
    let lim = limit.map(|p| GapLimitsFile { preset: "Test ladder".into(), source: "test".into(), effective_from: String::new(), limits: vec![GapLimit { bucket: format!("0{EN}1 month"), min_cumulative_gap_pct: p }] });
    build_gap_ladder(vec![(a.lazy(), spec(GapRole::Asset)), (l.lazy(), spec(GapRole::Liability))], &preset, "2026-10-31", true, lim.as_ref()).unwrap_or_else(|e| panic!("{e:#}"))
}


/// The Phase 4 V2 example through the real builder: A1 STD 100 to SMA, A2 SS 50 to DF, A3 STD 30 closed, A4 new STD 20.
fn v2_matrix() -> DataFrame {
    let frame = |keys: &[&str], bal: &[f64], cl: &[&str]| DataFrame::new(vec![Series::new("Actnum".into(), keys), Series::new("Balance".into(), bal), Series::new("CL_Status".into(), cl)]).unwrap();
    let prev = frame(&["A1", "A2", "A3"], &[100.0, 50.0, 30.0], &["STD", "SS", "STD"]);
    let curr = frame(&["A1", "A2", "A4"], &[100.0, 50.0, 20.0], &["SMA", "DF", "STD"]);
    let spec = ClMigrationSpec { key_col: "Actnum".into(), bal_col: "Balance".into(), cl_col: "CL_Status".into(), segment_col: None, measure: MigrationMeasure::Balance, pct_of_row: false, balance_basis: Default::default() };
    build_cl_migration_with(prev.lazy(), curr.lazy(), &spec, &ProvisionConfig::defaults().cl_code_map).unwrap_or_else(|e| panic!("{e:#}"))
}










fn ctx() -> Ctx {
    Ctx { divisor: 1.0, suffix: "Cr".into(), dq_green: Some(98.0), dq_amber: Some(95.0), ..Default::default() }
}

/// What a template makes, calculated: the derived table, or the tab itself, through the chart engine.
pub(crate) fn chart_of(t: Template, df: &DataFrame) -> (micro_parquet::charts::templates::Prepared, ChartData) {
    let p = prepare(t, df, &ctx()).unwrap_or_else(|e| panic!("{}: {e:#}", t.name()));
    let src = p.table.clone().unwrap_or_else(|| df.clone());
    let data = compute(src.lazy(), &p.spec).unwrap_or_else(|e| panic!("{}: {e:#}", t.name()));
    (p, data)
}

pub(crate) fn all_cases() -> Vec<(Template, DataFrame)> {
    vec![
        (Template::GapLadder, v3_ladder(Some(-10.0))),
        (Template::RepricingGap, repricing()),
        (Template::NiiBridge, mom_table()),
        (Template::ClMigration, v2_matrix()),
        (Template::ClassifiedTrend, classified()),
        (Template::IrrbbScenarios, irrbb_results()),
        (Template::LcrNsfr, lcr_summary()),
        (Template::EclByStage, ecl_summary()),
        (Template::DepositMix, mix()),
        (Template::RateProfile, rate_bands()),
        (Template::BehaviourProfile, history(8)),
        (Template::DataQuality, scorecard()),
    ]
}


// ------------------------------------------------------------------------------------------ tests

#[test]
fn c5_the_gap_ladder_of_the_v3_example_rings_the_breach_and_titles_the_turn() {
    let ladder = v3_ladder(Some(-10.0));
    assert_eq!(detect(&ladder), vec![Template::GapLadder]);
    let (p, d) = chart_of(Template::GapLadder, &ladder);
    // the form: linked panels, net gap columns over a cumulative gap % line, the ladder's own bucket order
    assert_eq!(p.spec.panels.len(), 2);
    assert_eq!((p.spec.panels[0].kind, p.spec.panels[1].kind), (ChartKind::Bar, ChartKind::Line));
    assert_eq!(d.categories, [format!("0{EN}1 month"), format!("1{EN}3 months"), "Over 3 months".to_string()], "the totals and the overdue row are not buckets");
    assert_eq!(p.finding, format!("Cumulative gap turns positive after 1{EN}3 months"));
    assert_eq!(chart_title(&d), p.finding, "the finding is the title");
    // the breach: ringed on bucket 1 and labelled (one decimal and the true minus on the limit, as in the reference image)
    assert_eq!(p.spec.limit_ticks.len(), 1);
    let tick = &p.spec.limit_ticks[0];
    assert_eq!((tick.category.as_str(), tick.value, tick.panel), (format!("0{EN}1 month").as_str(), -10.0, 1));
    assert_eq!(tick.breach, "Breach: \u{2212}16.7% vs \u{2212}10% limit");
    assert!(p.spec.limit_note.contains("minimum"));
    for l in all_sizes(&d, "gap ladder") {
        assert!(l.message.is_none(), "{:?}", l.message);
        assert_eq!(l.panels.len(), 2);
        let rings: Vec<_> = l.prims.iter().filter_map(|p| if let Prim::Outline { c, r, col, .. } = p { Some((*c, *r, *col)) } else { None }).collect();
        assert_eq!(rings.len(), 1, "one breach, one ring");
        // the ring is on bucket 1's point of the cumulative line
        let line = &l.panels[1];
        let band = line.plot.width() / 3.0;
        let want = egui::pos2(line.plot.left() + band * 0.5, line.coord(-16.666_666_666_666_668));
        assert!((rings[0].0.x - want.x).abs() < 1.0 && (rings[0].0.y - want.y).abs() < 1.0, "{:?} vs {:?}", rings[0].0, want);
        assert_eq!(rings[0].2, egui::Color32::from_rgb(0xd0, 0x3b, 0x3b), "the status colour");
        assert!(l.texts.iter().any(|x| x.role == Role::Reference && x.text == "Breach: \u{2212}16.7% vs \u{2212}10% limit"), "the breach is labelled");
        // a tick at the limit, across bucket 1's band
        let tick_y = line.coord(-10.0);
        assert!(l.prims.iter().any(|p| matches!(p, Prim::Line { a, b, w, col } if (a.y - tick_y).abs() < 0.6 && (b.y - tick_y).abs() < 0.6 && *w == t::REFERENCE_LINE_PX && *col == egui::Color32::from_rgb(0xd0, 0x3b, 0x3b))), "the limit tick");
        assert!(l.texts.iter().any(|x| x.role == Role::Reference && x.text.contains("minimum (ALCO limit)")), "the ticks are explained once");
        // the net gap columns: bucket 1 negative (down from zero), the others up
        let bars: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Bar && m.panel == 0).collect();
        assert_eq!(bars.len(), 3);
        assert!(bars[0].rect.top() >= l.panels[0].coord(0.0) - 0.5 && bars[1].rect.bottom() <= l.panels[0].coord(0.0) + 0.5);
    }
    // within its limit: no ring, no label, the tick stays
    let (p, _) = chart_of(Template::GapLadder, &v3_ladder(Some(-20.0)));
    assert!(p.spec.limit_ticks[0].breach.is_empty());
    // no limits file: no ticks and no note
    let (p, _) = chart_of(Template::GapLadder, &v3_ladder(None));
    assert!(p.spec.limit_ticks.is_empty() && p.spec.limit_note.is_empty());
    // the other finding: a ladder that never turns positive
    let df = DataFrame::new(vec![
        Series::new("Bucket".into(), ["A", "B", "Total"]),
        Series::new("Net gap".into(), [Some(-5.0), Some(-1.0), Some(-6.0)]),
        Series::new("Cumulative gap % of total assets".into(), [Some(-5.0), Some(-6.0), None]),
    ])
    .unwrap();
    assert_eq!(prepare(Template::GapLadder, &df, &ctx()).unwrap().finding, "Cumulative gap stays negative to B");
}

#[test]
fn c5_irrbb_scenarios_are_sorted_diverging_bars_with_the_outlier_line() {
    let r = irrbb_results();
    assert_eq!(detect(&r), vec![Template::IrrbbScenarios]);
    let (p, d) = chart_of(Template::IrrbbScenarios, &r);
    assert_eq!(p.finding, "Parallel up is the worst case at \u{2212}10.5% of Tier 1", "as the reference image 04_irrbb_scenarios.png");
    assert!(p.spec.horizontal);
    assert_eq!(d.categories, ["Parallel up", "Short up", "Steepener", "Flattener", "Short down", "Parallel down"], "sorted, worst first");
    let v: Vec<f64> = d.values[0].iter().map(|x| x.unwrap()).collect();
    assert!((v[0] + 10.49691).abs() < 1e-3 && (v[5] - 12.568416).abs() < 1e-3, "\u{394}EVE as % of Tier 1: {v:?}");
    assert_eq!((p.spec.ref_lines[0].value, p.spec.ref_lines[0].label.as_str()), (-15.0, "Outlier \u{2212}15%"));
    assert_eq!(p.spec.subtitle, "\u{394}EVE by BB scenario, % of Tier 1 capital \u{b7} outlier test at \u{2212}15%");
    assert_eq!(p.spec.footnote, "BRPD IRRBB guideline 2021, Table 2 shocks");
    for l in all_sizes(&d, "irrbb") {
        assert!(l.message.is_none());
        let bars: Vec<_> = l.marks.iter().filter(|m| m.kind == MarkKind::Bar).collect();
        assert_eq!(bars.len(), 6);
        let zero = l.panels[0].coord(0.0);
        assert!(bars[0].rect.right() <= zero + 0.5 && bars[5].rect.left() >= zero - 0.5, "negative bars go left of zero, positive right");
        assert!(l.texts.iter().any(|x| x.role == Role::Reference && x.text.contains("Outlier")), "the outlier line is labelled");
        assert!(l.texts.iter().any(|x| x.role == Role::Title && x.text.contains('\u{394}') || x.role == Role::Subtitle && x.text.contains('\u{394}')), "\u{394}EVE");
    }
    // with no loss at all the title says so
    let mut gains = irrbb_results();
    let all_gain = Series::new("EVE change".into(), [Some(1.0), Some(2.0), Some(3.0), Some(4.0), Some(5.0), Some(6.0), None, None, None, None]);
    gains.with_column(all_gain).unwrap();
    assert!(prepare(Template::IrrbbScenarios, &gains, &ctx()).unwrap().finding.starts_with("No scenario loses value"));
    // a result without its Tier 1 row is refused in plain words
    let no_t1 = irrbb_results().head(Some(8));
    let e = prepare(Template::IrrbbScenarios, &no_t1, &ctx()).err().unwrap().to_string();
    assert!(e.contains("Tier 1 capital"), "{e}");
}

#[test]
fn c5_the_nii_bridge_has_the_c0_bases_and_names_the_largest_effect() {
    let m = mom_table();
    assert_eq!(detect(&m), vec![Template::NiiBridge]);
    let (p, d) = chart_of(Template::NiiBridge, &m);
    let steps = waterfall_from_data(&d).expect("the bridge adds up");
    let bases: Vec<f64> = steps[1..4].iter().map(|s| s.base).collect();
    assert_eq!(bases, vec![80.0, 88.0, 93.0], "the C0 floating bases");
    assert_eq!((steps[0].value, steps[4].value), (80.0, 93.5));
    assert_eq!(p.finding, "Higher balances explain most of the +13.5 Cr rise in TD interest");
    // no effect above half: the largest is named instead
    let mut flat = mom_table();
    flat.with_column(Series::new("Volume effect".into(), [4.0])).unwrap();
    flat.with_column(Series::new("Rate effect".into(), [4.5])).unwrap();
    flat.with_column(Series::new("Mix effect".into(), [5.0])).unwrap();
    assert_eq!(prepare(Template::NiiBridge, &flat, &ctx()).unwrap().finding, "Mix is the largest effect behind the +13.5 Cr rise in TD interest");
    // a fall
    let mut fall = mom_table();
    fall.with_column(Series::new("Curr annual interest".into(), [70.0])).unwrap();
    fall.with_column(Series::new("Volume effect".into(), [-6.0])).unwrap();
    fall.with_column(Series::new("Rate effect".into(), [-3.0])).unwrap();
    fall.with_column(Series::new("Mix effect".into(), [-1.0])).unwrap();
    assert_eq!(prepare(Template::NiiBridge, &fall, &ctx()).unwrap().finding, "Lower balances explain most of the \u{2212}10.0 Cr fall in TD interest");
    // a bridge that does not add up is refused by the waterfall, not drawn
    let mut bad = mom_table();
    bad.with_column(Series::new("Mix effect".into(), [9.0])).unwrap();
    let (_, dbad) = chart_of(Template::NiiBridge, &bad);
    assert!(waterfall_from_data(&dbad).unwrap_err().to_string().contains("does not add up"));
    for l in all_sizes(&d, "bridge") {
        assert_eq!(l.marks.len(), 5);
    }
}

#[test]
fn c5_the_cl_migration_title_names_the_largest_downgrade() {
    let m = v2_matrix();
    assert_eq!(detect(&m), vec![Template::ClMigration]);
    let (p, d) = chart_of(Template::ClMigration, &m);
    assert_eq!(p.finding, "100.0% of SS moved to DF", "the V2 example");
    assert_eq!(p.spec.kind, ChartKind::Heatmap);
    // the heatmap is the % of each previous grade: its rows add up to 100 and the grade order is the CL order
    let h = micro_parquet::charts::data::heatmap(&d, micro_parquet::charts::spec::Transform::None);
    assert_eq!(h.cols, ["STD", "SMA", "SS", "DF", "BL", "Closed/Repaid"]);
    assert_eq!(h.rows, ["STD", "SMA", "SS", "DF", "BL"].iter().filter(|g| d.series.iter().any(|s| s == *g)).map(|s| s.to_string()).collect::<Vec<_>>());
    for r in 0..h.rows.len() {
        let sum: f64 = h.cells[r].iter().flatten().sum();
        assert!((sum - 100.0).abs() < 1e-6, "row {r}: {sum}");
    }
    for l in all_sizes(&d, "migration") {
        assert!(l.message.is_none());
        let cells = l.marks.iter().filter(|m| m.kind == MarkKind::Cell).count();
        assert_eq!(cells, h.rows.len() * 6, "every pair is a cell");
    }
    assert!(chart_html(&d, &opts()).is_ok());
    // no downgrade at all
    let stay = DataFrame::new(vec![Series::new("Previous grade".into(), ["STD", "SS"]), Series::new("STD".into(), [10.0, 0.0]), Series::new("SS".into(), [0.0, 5.0]), Series::new("Closed/Repaid".into(), [0.0, 0.0]), Series::new("Total".into(), [10.0, 5.0])]).unwrap();
    assert_eq!(prepare(Template::ClMigration, &stay, &ctx()).unwrap().finding, "No loan moved to a worse grade");
}

#[test]
fn c5_the_other_templates_give_their_form_and_their_finding() {
    // classified trend: stacked columns by grade plus an NPL % panel
    let (p, d) = chart_of(Template::ClassifiedTrend, &classified());
    assert_eq!(p.finding, "NPL ratio rose to 7.0%");
    assert_eq!((p.spec.kind, p.spec.stack, p.spec.panels.len()), (ChartKind::Bar, micro_parquet::charts::spec::Stack::Stacked, 2));
    assert_eq!(d.categories, ["Aug 2026", "Sep 2026", "Oct 2026"]);
    assert_eq!(d.series.len(), 4, "SMA, SS, DF and BL: Standard is not stacked");
    // yield curves: lines by curve date, the latest highlighted
    let y = yield_curves(&curves()).unwrap();
    assert_eq!(y.finding, "The curve rose by 80 bp at 5Y since 30 Jun 2026");
    assert_eq!((y.spec.kind, y.spec.highlight.as_deref()), (ChartKind::Line, Some("31 Oct 2026")));
    let d = compute(y.table.clone().unwrap().lazy(), &y.spec).unwrap();
    assert_eq!((d.categories.len(), d.series.len()), (4, 3));
    assert_eq!(yield_curves(&curves()[2..]).unwrap().finding, "One curve, dated 31 Oct 2026");
    assert!(yield_curves(&[]).unwrap_err().to_string().contains("no curve"));
    // LCR and NSFR: bars with the minimum ticked
    let (p, d) = chart_of(Template::LcrNsfr, &lcr_summary());
    assert_eq!(p.finding, "LCR 133.7% against a 100% minimum");
    assert_eq!((d.categories.clone(), p.spec.limit_ticks.len()), (vec!["LCR".to_string(), "NSFR".to_string()], 2));
    assert!(p.spec.limit_ticks.iter().all(|t| t.breach.is_empty()));
    let mut low = lcr_summary();
    low.with_column(Series::new("Value".into(), [Some(3_500.0), Some(1_000.0), Some(500.0), Some(0.0), Some(0.0), Some(5_000.0), Some(92.0), Some(100.0), None, Some(108.2), Some(100.0), None])).unwrap();
    // the tool's own status row says it is below the minimum (the chart does not do that arithmetic)
    low.with_column(Series::new("Note".into(), ["", "", "", "", "", "", "", "at least", "Below the minimum", "", "at least", "Meets the minimum"])).unwrap();
    assert_eq!(prepare(Template::LcrNsfr, &low, &ctx()).unwrap().spec.limit_ticks[0].breach, "Breach: 92.0% vs 100% minimum");
    // ECL by stage
    let (p, d) = chart_of(Template::EclByStage, &ecl_summary());
    assert_eq!(p.finding, "Stage 3 holds 66.7% of ECL");
    assert_eq!(d.categories, ["Stage 1", "Stage 2", "Stage 3"]);
    assert_eq!(p.spec.panels.len(), 3, "EAD, ECL and coverage %: panels, never a second axis");
    assert_eq!(d.measure(2).unwrap().values[0][2], Some(40.0), "stage 3 coverage 40%");
    // deposit / loan mix
    let (p, _) = chart_of(Template::DepositMix, &mix());
    assert_eq!(p.finding, "Term deposit drove growth");
    let t = p.table.as_ref().expect("the months are written as labels, so the chart has its own table");
    assert_eq!(p.tab_title, "Chart: deposit mix");
    let months: Vec<String> = t.column("Month").unwrap().str().unwrap().into_iter().flatten().map(String::from).collect();
    assert_eq!(&months[..3], ["Aug 2026", "Aug 2026", "Aug 2026"], "months read Aug 2026, not 2026-08");
    // rate profile
    let (p, d) = chart_of(Template::RateProfile, &rate_bands());
    assert_eq!(p.finding, "62.5% of balances sit at 8%\u{2013}9%");
    assert_eq!(d.categories.first().map(String::as_str), Some("08. 7%\u{2013}8%"), "band order");
    // behaviour profile: the core level as a band
    let (p, _) = chart_of(Template::BehaviourProfile, &history(8));
    assert!(p.finding.starts_with("Core balance ") && p.finding.ends_with("% of the latest"), "{}", p.finding);
    assert_eq!(p.spec.ref_bands.len(), 1);
    let e = prepare(Template::BehaviourProfile, &history(2), &ctx()).err().unwrap().to_string();
    assert!(e.contains("at least 3 month-ends"), "{e}");
    // data quality
    let (p, d) = chart_of(Template::DataQuality, &scorecard());
    assert_eq!(p.finding, "1 rule below Amber");
    assert_eq!(d.categories.len(), 3, "the overall row and the control total are not bars");
    assert_eq!(p.spec.ref_lines.len(), 2);
    assert_eq!((p.spec.ref_lines[0].status, p.spec.ref_lines[1].status), (micro_parquet::charts::spec::Status::Good, micro_parquet::charts::spec::Status::Warning));
    // repricing gap
    let (p, _) = chart_of(Template::RepricingGap, &repricing());
    assert_eq!(p.finding, "Largest repricing gap in >12m");
}

#[test]
fn c5_each_template_is_detected_only_on_its_own_tool_and_says_what_is_missing_otherwise() {
    for (t, df) in all_cases() {
        let found = detect(&df);
        assert!(found.contains(&t), "{}: {:?}", t.name(), found);
        assert!(fits(t, &df));
    }
    // a plain file tab fits none of the tool templates
    let plain = DataFrame::new(vec![Series::new("Actnum".into(), ["A"]), Series::new("Balance".into(), [1.0])]).unwrap();
    assert!(detect(&plain).is_empty());
    let e = prepare(Template::GapLadder, &plain, &ctx()).err().unwrap().to_string();
    assert!(e.contains("no column 'Bucket'") && e.contains("Liquidity gap ladder"), "{e}");
    let e = prepare(Template::IrrbbScenarios, &DataFrame::new(vec![Series::new("Scenario".into(), ["x"]), Series::new("EVE change".into(), ["text"]), Series::new("Loss % of Tier 1".into(), [1.0])]).unwrap(), &ctx()).err().unwrap().to_string();
    assert!(e.contains("is str") || e.contains("not a number"), "{e}");
    for t in Template::all() {
        assert!(!t.use_when().is_empty() && !t.avoid_when().is_empty() && !t.form().is_empty() && !t.source().is_empty());
        assert_eq!(Template::from_id(t.id()), Some(*t));
    }
    assert_eq!(Template::all().len(), 13);
}

#[test]
fn c5_every_template_draws_in_all_three_renderers_and_a_takeaway_replaces_the_finding() {
    for (t, df) in all_cases() {
        let (p, d) = chart_of(t, &df);
        // in-app: all three sizes, light and dark, by the design rules
        let _ = all_sizes(&d, t.name());
        // HTML: the page is made (no refusal)
        chart_html(&d, &opts()).unwrap_or_else(|e| panic!("{}: {e:#}", t.name()));
        // the title is the finding; a typed takeaway replaces it
        assert_eq!(chart_title(&d), p.finding, "{}", t.name());
        let mut spec: ChartSpec = p.spec.clone();
        spec.takeaway = "My own sentence".into();
        let mut d2 = d.clone();
        d2.spec = spec;
        assert_eq!(chart_title(&d2), "My own sentence", "{}", t.name());
        // every template round-trips through the saved-chart file
        let dir = std::env::temp_dir().join(format!("mp-c5-{}-{}", std::process::id(), t.id()));
        let path = micro_parquet::charts::library::save_chart(&dir, t.name(), &p.spec).unwrap();
        let (_, back) = micro_parquet::charts::library::load_chart(&path).unwrap();
        assert_eq!(back, p.spec, "{}", t.name());
        std::fs::remove_dir_all(&dir).ok();
    }
    // a chart saved before the limit ticks existed loads unchanged
    let old: ChartSpec = serde_json::from_str(r#"{"category":"Month","value":"Balance"}"#).unwrap();
    assert!(old.limit_ticks.is_empty() && old.limit_note.is_empty());
    let layout = lay(&compute(mix().lazy(), &prepare(Template::DepositMix, &mix(), &ctx()).unwrap().spec).unwrap(), 900.0, 560.0, true);
    check(&layout, "dark");
}

#[test]
fn c5_limit_ticks_reach_the_html_and_the_excel_charts() {
    let (p, d) = chart_of(Template::GapLadder, &v3_ladder(Some(-10.0)));
    let out = micro_parquet::charts::echarts::chart_pair(&d, &opts()).unwrap();
    let series = out.light["series"].as_array().unwrap();
    let limit = series.iter().find(|s| s["name"] == "Limit").expect("the limit ticks");
    assert_eq!(limit["data"][0]["value"][1].as_f64(), Some(-10.0));
    assert_eq!(limit["data"][0]["itemStyle"]["color"], "#d03b3b");
    let ring = series.iter().find(|s| s["name"] == "Breach").expect("the ring");
    assert_eq!(ring["data"][0]["label"]["formatter"], "Breach: \u{2212}16.7% vs \u{2212}10% limit");
    assert!(out.light["graphic"].to_string().contains("minimum (ALCO limit)"), "the note is under the chart");
    // Excel: a Limit and a Breach column in the table, and a series of dash markers
    let path = std::env::temp_dir().join(format!("mp-c5-{}.xlsx", std::process::id()));
    micro_parquet::charts::xlsx::write_chart_xlsx(&path, &d, &opts()).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() > 1000);
    std::fs::remove_file(&path).ok();
    assert!(!p.spec.limit_ticks.is_empty());
}
