//! IRRBB: six scenarios, ΔEVE and ΔNII (Phase 5 R3), after the Bangladesh Bank *Guidelines on
//! Interest Rate Risk in the Banking Book*, 13 April 2021 (§13.1 outlier test, §14.2 scenarios,
//! §15 measurement, Table 2 shocks, Appendix 2 Table B).
//!
//! Rates are decimals inside the maths (0.10 = 10%), shocks are in basis points, times are in
//! years (days ÷ 365). Signs: assets positive, liabilities negative; "EVE change" is negative for a
//! loss; "Loss" is a positive number.

use crate::behaviour::BehaviourFile;
use crate::betas::BetaRow;
use crate::cashflow::{build_rows_for_irrbb, ladder_from_rows, BucketAmounts, CashFlowMode, CfInputSpec, CfRow, CfSettings, LadderResult, Scenario, Side, IRRBB_BUCKETS};
use crate::curves::{curve_file_name, YieldCurve};
use crate::rules::{CurrencyShock, RuleFile, RuleMeta, ShockFile};
use anyhow::{anyhow, bail, Result};
use chrono::NaiveDate;
use polars::prelude::*;

/// How the curve's rates are quoted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Compounding {
    /// Annual compounding: the rate used in e^(−R·t) is ln(1 + y).
    #[default]
    Annual,
    /// The curve already gives continuously compounded rates.
    Continuous,
}

impl Compounding {
    pub fn label(self) -> &'static str {
        match self {
            Compounding::Annual => "Annual compounding (converted with ln(1 + y))",
            Compounding::Continuous => "Continuous",
        }
    }
    /// The continuously compounded rate (decimal) of a curve rate in percent.
    pub fn continuous(self, y_pct: f64) -> f64 {
        let y = y_pct / 100.0;
        match self {
            Compounding::Annual => (1.0 + y).ln(),
            Compounding::Continuous => y,
        }
    }
}

/// The shock of a scenario at time `t` (years), in basis points, for a currency's sizes (BB §14.2):
/// with s(t) = e^(−t/x) — parallel ±R_par; short ±R_short·s(t); steepener −0.65·R_short·s(t) +
/// 0.9·R_long·(1 − s(t)); flattener +0.8·R_short·s(t) − 0.6·R_long·(1 − s(t)). The base case is 0.
pub fn shock_bp(scn: Scenario, t: f64, cs: &CurrencyShock, x: f64) -> f64 {
    let s = (-t.max(0.0) / x).exp();
    match scn {
        Scenario::Base => 0.0,
        Scenario::ParallelUp => cs.parallel_bp,
        Scenario::ParallelDown => -cs.parallel_bp,
        Scenario::ShortUp => cs.short_bp * s,
        Scenario::ShortDown => -cs.short_bp * s,
        Scenario::Steepener => -0.65 * cs.short_bp * s + 0.9 * cs.long_bp * (1.0 - s),
        Scenario::Flattener => 0.8 * cs.short_bp * s - 0.6 * cs.long_bp * (1.0 - s),
    }
}

/// The curve's rate in percent at `t` years: linear between its points (tenors in days, t × 365),
/// flat beyond the first and last point.
pub fn curve_rate_pct(curve: &YieldCurve, t: f64) -> Result<f64> {
    let pts = &curve.points;
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
        bail!("The curve of {} has no points.", curve.date.format("%Y-%m-%d"));
    };
    let d = t * 365.0;
    if d <= first.tenor_days as f64 {
        return Ok(first.ftp_rate);
    }
    if d >= last.tenor_days as f64 {
        return Ok(last.ftp_rate);
    }
    for w in pts.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if d <= b.tenor_days as f64 {
            let span = (b.tenor_days - a.tenor_days) as f64;
            return Ok(a.ftp_rate + (b.ftp_rate - a.ftp_rate) * (d - a.tenor_days as f64) / span);
        }
    }
    Ok(last.ftp_rate)
}

/// The discount factor e^(−R·t) of the curve at `t`.
pub fn discount_factor(curve: &YieldCurve, t: f64, comp: Compounding) -> Result<f64> {
    let r = comp.continuous(curve_rate_pct(curve, t)?);
    Ok((-r * t).exp())
}

/// The curve of `currency` in force at `cutoff` (the latest dated on or before it).
pub fn pick_curve<'a>(curves: &'a [YieldCurve], currency: &str, cutoff: NaiveDate) -> Result<&'a YieldCurve> {
    let want = currency.trim().to_uppercase();
    let mine: Vec<&YieldCurve> = curves.iter().filter(|c| c.currency.trim().to_uppercase() == want).collect();
    if mine.is_empty() {
        bail!("IRRBB: there is no {} yield curve in portable_data/curves. Add a {} curve in Banking > Yield curves, dated on or before the cutoff {}.", want, want, cutoff.format("%Y-%m-%d"));
    }
    mine.iter().filter(|c| c.date <= cutoff).max_by_key(|c| c.date).copied().ok_or_else(|| {
        let earliest = mine.iter().map(|c| c.date).min().map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
        anyhow!("IRRBB: the earliest {} curve is dated {}, after the cutoff {}. Add a {} curve dated on or before the cutoff.", want, earliest, cutoff.format("%Y-%m-%d"), want)
    })
}

/// What the EVE of one currency needs.
pub struct EveSetup<'a> {
    pub curve: &'a YieldCurve,
    pub shock: &'a CurrencyShock,
    pub decay_x: f64,
    pub compounding: Compounding,
    /// The post-shock floor in percent (same basis as the curve); off by default (the guideline sets none).
    pub floor_pct: Option<f64>,
}

/// The present value of a ladder at the bucket midpoints for a scenario: each bucket's net cash
/// flow × e^(−(R + ΔR)·t), with R the curve rate (continuous) and ΔR the scenario's shock at the
/// midpoint, the shocked rate not below the floor when there is one.
pub fn bucket_pvs(net: &[f64; 19], scn: Scenario, setup: &EveSetup) -> Result<[f64; 19]> {
    let mut out = [0.0; 19];
    for (k, b) in IRRBB_BUCKETS.iter().enumerate() {
        let r = setup.compounding.continuous(curve_rate_pct(setup.curve, b.midpoint)?);
        let mut shocked = r + shock_bp(scn, b.midpoint, setup.shock, setup.decay_x) / 10_000.0;
        if let Some(f) = setup.floor_pct {
            shocked = shocked.max(setup.compounding.continuous(f));
        }
        out[k] = net[k] * (-shocked * b.midpoint).exp();
    }
    Ok(out)
}

// ----------------------------------------------------------------------------------------- ΔNII

/// What ΔNII needs besides the rows (the Phase 4 V5 options).
pub struct NiiSetup<'a> {
    pub cutoff: NaiveDate,
    pub betas: &'a [BetaRow],
    /// A floor on the current rate, in percent: a fall stops at it.
    pub floor_pct: Option<f64>,
}

/// The weight (1 − t for a row that reprices within a year, 1 for one that reprices now or has no
/// date, 0 beyond a year) and the time t of the row's repricing. The effective repricing date is
/// the earlier of the repricing date and the maturity.
pub fn repricing_weight(row: &CfRow, cutoff: NaiveDate) -> (f64, f64) {
    let eff = match (row.reprice, row.maturity) {
        (Some(r), Some(m)) => Some(r.min(m)),
        (Some(r), None) => Some(r),
        (None, m) => m,
    };
    let Some(eff) = eff else {
        return (1.0, 0.0);
    };
    let rem = (eff - cutoff).num_days();
    if rem <= 0 {
        (1.0, 0.0)
    } else if rem > 365 {
        (0.0, rem as f64 / 365.0)
    } else {
        ((365.0 - rem as f64) / 365.0, rem as f64 / 365.0)
    }
}

/// The ΔNII of one row under a scenario: the balance × weight × the change of its rate, with the
/// shock at the row's repricing time, passed through its product's beta (up for a rise, down for a
/// fall), a fall stopped at the floor; positive for income on an asset, negative for an increase
/// in the expense on a liability.
pub fn row_nii(row: &CfRow, scn: Scenario, shock: &CurrencyShock, decay_x: f64, setup: &NiiSetup) -> f64 {
    let (weight, t) = repricing_weight(row, setup.cutoff);
    if weight == 0.0 {
        return 0.0;
    }
    let bp = shock_bp(scn, t, shock, decay_x);
    if bp == 0.0 {
        return 0.0;
    }
    let code = crate::behaviour::normalise_product(&row.product);
    let beta = setup.betas.iter().find(|b| b.product == code).map(|b| if bp < 0.0 { b.down } else { b.up }).unwrap_or(1.0);
    let mut delta_pct = bp / 100.0 * beta;
    if let Some(f) = setup.floor_pct {
        if delta_pct < 0.0 {
            let rate = if row.rate_pct.is_finite() { row.rate_pct } else { 0.0 };
            delta_pct = delta_pct.max(f - rate).min(0.0);
        }
    }
    row.principal * weight * delta_pct / 100.0 * row.side.sign()
}

// ----------------------------------------------------------------------------------------- the run

/// Everything the six-scenario run takes besides the tabs.
#[derive(Clone, Debug)]
pub struct IrrbbSpec {
    pub cutoff: NaiveDate,
    pub mode: CashFlowMode,
    /// "Treat blank maturity as O/N".
    pub blank_maturity_on: bool,
    pub tier1: f64,
    pub compounding: Compounding,
    /// The post-shock floor in percent, off by default.
    pub post_shock_floor_pct: Option<f64>,
    /// A floor on current rates for ΔNII, in percent.
    pub nii_floor_pct: Option<f64>,
    pub shocks: RuleFile<ShockFile>,
    /// Every curve of the store; the run picks the one of each currency in force at the cutoff.
    pub curves: Vec<YieldCurve>,
    pub behaviour: Option<(String, BehaviourFile)>,
    pub betas: Vec<BetaRow>,
    /// The name of the beta file the `betas` come from, "" for none (for the results footer).
    pub beta_file: String,
}

/// One scenario's result.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioResult {
    pub scenario: Scenario,
    /// The sum of the currencies' EVE changes (negative for a loss).
    pub eve_change: f64,
    /// The sum of each currency's loss max(0, −EVE change): no offsets between currencies (BB §15.32).
    pub loss: f64,
    pub nii: f64,
    /// Currency, EVE change, loss.
    pub by_currency: Vec<(String, f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct IrrbbOutput {
    /// The base-case ladder (the ladder of the cash-flow tool).
    pub base: LadderResult,
    /// Base PV per bucket for each currency.
    pub base_pv: Vec<(String, [f64; 19])>,
    pub scenarios: Vec<ScenarioResult>,
    pub tier1: f64,
    pub max_loss: f64,
    pub max_loss_scenario: Option<Scenario>,
    pub max_loss_pct: f64,
    pub outlier_pct: f64,
    pub outlier: bool,
    pub shock_meta: RuleMeta,
    /// Currency and the curve file used.
    pub curve_files: Vec<(String, String)>,
    pub behaviour_file: Option<String>,
    pub compounding: Compounding,
    /// Every input of the run that is not a file, as it was typed: label and text (cutoff, mode, options,
    /// floors), so the run can be repeated from the footer.
    pub inputs: Vec<(String, String)>,
    pub beta_file: String,
    pub notes: Vec<String>,
}

impl IrrbbOutput {
    pub fn base_eve(&self, currency: &str) -> Option<f64> {
        self.base_pv.iter().find(|(c, _)| c == currency).map(|(_, pv)| pv.iter().sum())
    }
}

/// The six scenarios on rows already parsed.
pub fn run_rows(rows: &[CfRow], spec: &IrrbbSpec) -> Result<IrrbbOutput> {
    if !(spec.tier1.is_finite() && spec.tier1 > 0.0) {
        bail!("IRRBB: type the Tier 1 capital (above 0), in the same unit as the balances (BDT).");
    }
    crate::rules::check_shocks(&spec.shocks.data, &spec.shocks.meta.file)?;
    let behaviour = spec.behaviour.as_ref().map(|b| &b.1);
    let settings = |scenario| CfSettings { cutoff: spec.cutoff, mode: spec.mode, scenario, blank_maturity_on: spec.blank_maturity_on };
    let base = ladder_from_rows(rows, &settings(Scenario::Base), behaviour)?;
    let x = spec.shocks.data.decay_x;
    // each currency: its shocks and its curve
    let mut base_pv: Vec<(String, [f64; 19])> = Vec::new();
    let mut curve_files: Vec<(String, String)> = Vec::new();
    let mut setups: Vec<(String, &CurrencyShock, &YieldCurve)> = Vec::new();
    let mut currencies: Vec<String> = base.currencies.iter().map(|c| c.0.clone()).collect();
    for r in rows {
        if !currencies.contains(&r.currency) {
            currencies.push(r.currency.clone());
        }
    }
    for c in &currencies {
        let shock = spec.shocks.data.for_currency(c, &spec.shocks.meta.file)?;
        let curve = pick_curve(&spec.curves, c, spec.cutoff)?;
        curve_files.push((c.clone(), format!("curves/{} ({})", curve_file_name(curve.date), curve.currency.trim())));
        crate::rules::note_used_file("Yield curve", &format!("curves/{}", curve_file_name(curve.date)), &curve.date.format("%Y-%m-%d").to_string(), &curve.source, false);
        setups.push((c.clone(), shock, curve));
    }
    let eve = |c: &str| -> Option<EveSetup> {
        setups.iter().find(|s| s.0 == c).map(|(_, shock, curve)| EveSetup { curve, shock, decay_x: x, compounding: spec.compounding, floor_pct: spec.post_shock_floor_pct })
    };
    for (c, amounts) in &base.currencies {
        let setup = eve(c).ok_or_else(|| anyhow!("IRRBB: no setup for {}", c))?;
        base_pv.push((c.clone(), bucket_pvs(&amounts.net(), Scenario::Base, &setup_no_floor(&setup))?));
    }
    let nii_setup = NiiSetup { cutoff: spec.cutoff, betas: &spec.betas, floor_pct: spec.nii_floor_pct };
    let mut scenarios = Vec::new();
    for scn in Scenario::SIX {
        let ladder = ladder_from_rows(rows, &settings(scn), behaviour)?;
        let mut by_currency = Vec::new();
        for (c, amounts) in &ladder.currencies {
            let setup = eve(c).ok_or_else(|| anyhow!("IRRBB: no setup for {}", c))?;
            let shocked: f64 = bucket_pvs(&amounts.net(), scn, &setup)?.iter().sum();
            let base_total: f64 = base_pv.iter().find(|(bc, _)| bc == c).map(|(_, pv)| pv.iter().sum()).unwrap_or(0.0);
            let change = shocked - base_total;
            by_currency.push((c.clone(), change, (-change).max(0.0)));
        }
        // a currency that has flows only in the base case (all of them matured away) still has a base value
        for (c, pv) in &base_pv {
            if !by_currency.iter().any(|b| b.0 == *c) {
                let change = -pv.iter().sum::<f64>();
                by_currency.push((c.clone(), change, (-change).max(0.0)));
            }
        }
        let mut nii = 0.0;
        for r in rows {
            let shock = spec.shocks.data.for_currency(&r.currency, &spec.shocks.meta.file)?;
            nii += row_nii(r, scn, shock, x, &nii_setup);
        }
        scenarios.push(ScenarioResult { scenario: scn, eve_change: by_currency.iter().map(|b| b.1).sum(), loss: by_currency.iter().map(|b| b.2).sum(), nii, by_currency });
    }
    let (max_loss, max_loss_scenario) = scenarios.iter().fold((0.0f64, None), |acc, s| if s.loss > acc.0 { (s.loss, Some(s.scenario)) } else { acc });
    let max_loss_pct = max_loss / spec.tier1 * 100.0;
    let outlier_pct = spec.shocks.data.outlier_pct;
    let mut notes = base.notes.clone();
    if currencies.len() > 1 {
        notes.push("Several currencies: each runs with its own curve and shocks, and the loss of a scenario is the sum of each currency's loss with no offsets between currencies (BB \u{a7}15.32).".to_string());
    }
    Ok(IrrbbOutput {
        base,
        base_pv,
        scenarios,
        tier1: spec.tier1,
        max_loss,
        max_loss_scenario,
        max_loss_pct,
        outlier_pct,
        // the guideline's test: a loss of 15% of Tier 1 capital or more
        outlier: max_loss_pct >= outlier_pct - 1e-9,
        shock_meta: spec.shocks.meta.clone(),
        curve_files,
        behaviour_file: spec.behaviour.as_ref().map(|b| format!("behaviour/{}", b.0)),
        compounding: spec.compounding,
        inputs: vec![
            ("Cutoff date".to_string(), spec.cutoff.format("%Y-%m-%d").to_string()),
            ("Cash flows".to_string(), spec.mode.label().to_string()),
            ("Treat blank maturity as O/N".to_string(), if spec.blank_maturity_on { "Yes" } else { "No" }.to_string()),
            ("Post-shock floor".to_string(), spec.post_shock_floor_pct.map(|f| format!("{} %", f)).unwrap_or_else(|| "off".to_string())),
            ("\u{394}NII floor on current rates".to_string(), spec.nii_floor_pct.map(|f| format!("{} %", f)).unwrap_or_else(|| "off".to_string())),
        ],
        beta_file: spec.beta_file.clone(),
        notes,
    })
}

/// The base case is discounted at the curve with no floor (a floor belongs to the shocked rates).
fn setup_no_floor<'a>(s: &EveSetup<'a>) -> EveSetup<'a> {
    EveSetup { curve: s.curve, shock: s.shock, decay_x: s.decay_x, compounding: s.compounding, floor_pct: None }
}

/// The six scenarios on tabs.
pub fn run_irrbb(inputs: Vec<(LazyFrame, CfInputSpec)>, spec: &IrrbbSpec) -> Result<IrrbbOutput> {
    let rows = build_rows_for_irrbb(inputs, &CfSettings { cutoff: spec.cutoff, mode: spec.mode, scenario: Scenario::Base, blank_maturity_on: spec.blank_maturity_on }, spec.behaviour.as_ref().map(|b| &b.1), spec.nii_floor_pct.is_some())?;
    run_rows(&rows, spec)
}

// ----------------------------------------------------------------------------------- the tables

pub const RESULT_COLUMNS: [&str; 6] = ["Scenario", "EVE change", "Loss", "Loss % of Tier 1", "\u{394}NII", "Note"];

fn side_note(base_eve: &[(String, f64)]) -> String {
    base_eve.iter().map(|(c, v)| format!("{} {:.2}", c, v)).collect::<Vec<_>>().join(", ")
}

/// "IRRBB results": one row per scenario, then the footer: the largest loss, the outlier test and
/// the files used.
pub fn results_frame(o: &IrrbbOutput) -> Result<DataFrame> {
    let mut sc: Vec<String> = Vec::new();
    let (mut eve, mut loss, mut pct, mut nii): (Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>) = Default::default();
    let mut note: Vec<String> = Vec::new();
    for s in &o.scenarios {
        sc.push(s.scenario.label().to_string());
        eve.push(Some(s.eve_change));
        loss.push(Some(s.loss));
        pct.push(Some(s.loss / o.tier1 * 100.0));
        nii.push(Some(s.nii));
        note.push(String::new());
    }
    let mut footer = |label: &str, loss_v: Option<f64>, pct_v: Option<f64>, text: String| {
        sc.push(label.to_string());
        eve.push(None);
        loss.push(loss_v);
        pct.push(pct_v);
        nii.push(None);
        note.push(text);
    };
    footer("Maximum loss", Some(o.max_loss), Some(o.max_loss_pct), o.max_loss_scenario.map(|s| s.label().to_string()).unwrap_or_else(|| "no scenario loses".to_string()));
    footer(&format!("Outlier (max loss \u{2265} {}% of Tier 1)", o.outlier_pct), None, None, if o.outlier { "Yes".to_string() } else { "No".to_string() });
    footer("Tier 1 capital", None, None, format!("{:.2}", o.tier1));
    for (k, v) in &o.inputs {
        footer(k, None, None, v.clone());
    }
    footer("Base EVE", None, None, side_note(&o.base_pv.iter().map(|(c, pv)| (c.clone(), pv.iter().sum::<f64>())).collect::<Vec<_>>()));
    footer("Rule file", None, None, o.shock_meta.file.clone());
    footer("Rule source", None, None, o.shock_meta.source.clone());
    footer("Curve", None, None, o.curve_files.iter().map(|(c, f)| format!("{}: {}", c, f)).collect::<Vec<_>>().join("; "));
    footer("Compounding", None, None, o.compounding.label().to_string());
    footer("Behaviour profile", None, None, o.behaviour_file.clone().unwrap_or_else(|| "none".to_string()));
    footer("Beta file", None, None, if o.beta_file.is_empty() { "none".to_string() } else { format!("betas/{}", o.beta_file) });
    if o.shock_meta.sample {
        footer("Sample parameters", None, None, "The shock file is marked sample: confirm the numbers before they are used in a return.".to_string());
    }
    Ok(DataFrame::new(vec![
        Series::new(RESULT_COLUMNS[0].into(), sc),
        Series::new(RESULT_COLUMNS[1].into(), eve),
        Series::new(RESULT_COLUMNS[2].into(), loss),
        Series::new(RESULT_COLUMNS[3].into(), pct),
        Series::new(RESULT_COLUMNS[4].into(), nii),
        Series::new(RESULT_COLUMNS[5].into(), note),
    ])?)
}

pub const CASHFLOW_COLUMNS: [&str; 9] = ["Currency", "Bucket", "Midpoint (years)", "Assets", "Liabilities", "Net", "Cumulative net", "Discount factor", "Base PV"];

/// "IRRBB cash flows": the cash-flow ladder with the base present value of each bucket, and a
/// "Base EVE" row for each currency.
pub fn cashflows_frame(o: &IrrbbOutput) -> Result<DataFrame> {
    let (mut cur, mut bucket, mut mid, mut a, mut l, mut n, mut c, mut df, mut pv): (Vec<String>, Vec<String>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>) = Default::default();
    for (ccy, amounts) in &o.base.currencies {
        let net = amounts.net();
        let pvs = o.base_pv.iter().find(|(x, _)| x == ccy).map(|(_, p)| *p).unwrap_or([0.0; 19]);
        let mut run = 0.0;
        for (i, b) in IRRBB_BUCKETS.iter().enumerate() {
            run += net[i];
            cur.push(ccy.clone());
            bucket.push(b.label.to_string());
            mid.push(Some(b.midpoint));
            a.push(Some(amounts.assets[i]));
            l.push(Some(amounts.liabilities[i]));
            n.push(Some(net[i]));
            c.push(Some(run));
            df.push(if net[i] != 0.0 { Some(pvs[i] / net[i]) } else { None });
            pv.push(Some(pvs[i]));
        }
        cur.push(ccy.clone());
        bucket.push("Base EVE".to_string());
        mid.push(None);
        a.push(None);
        l.push(None);
        n.push(None);
        c.push(None);
        df.push(None);
        pv.push(Some(pvs.iter().sum()));
    }
    for (ccy, count, total) in &o.base.excluded {
        cur.push(ccy.clone());
        bucket.push(crate::cashflow::excluded_label(*count));
        mid.push(None);
        a.push(Some(*total));
        l.push(None);
        n.push(None);
        c.push(None);
        df.push(None);
        pv.push(None);
    }
    Ok(DataFrame::new(vec![
        Series::new(CASHFLOW_COLUMNS[0].into(), cur),
        Series::new(CASHFLOW_COLUMNS[1].into(), bucket),
        Series::new(CASHFLOW_COLUMNS[2].into(), mid),
        Series::new(CASHFLOW_COLUMNS[3].into(), a),
        Series::new(CASHFLOW_COLUMNS[4].into(), l),
        Series::new(CASHFLOW_COLUMNS[5].into(), n),
        Series::new(CASHFLOW_COLUMNS[6].into(), c),
        Series::new(CASHFLOW_COLUMNS[7].into(), df),
        Series::new(CASHFLOW_COLUMNS[8].into(), pv),
    ])?)
}

/// `BucketAmounts` of a ladder for a currency (a convenience for callers and tests).
pub fn amounts_of<'a>(l: &'a LadderResult, currency: &str) -> Option<&'a BucketAmounts> {
    l.for_currency(currency)
}

/// A rule's side, for messages.
pub fn side_label(s: Side) -> &'static str {
    match s {
        Side::Asset => "asset",
        Side::Liability => "liability",
    }
}
