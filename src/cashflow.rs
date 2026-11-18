//! The cash-flow engine of the IRRBB tools (Phase 5 R1): dated cash flows from the cutoff date for
//! each row of a book, slotted into the 19 buckets of the Bangladesh Bank IRRBB guideline (Table 3).
//! Pure calculation, no UI.
//!
//! Signs: assets and inflows are positive, liabilities and outflows negative. Time in years is the
//! days from the cutoff date divided by 365. Amounts are BDT or BDT-equivalent (no currency conversion).

use crate::behaviour::{normalise_product, BehaviourFile, BehaviourProduct};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, Months, NaiveDate};
use polars::prelude::*;

/// One bucket of BB Table 3: its label, upper bound in years (each bucket includes its upper
/// bound) and midpoint in years.
#[derive(Clone, Copy, Debug)]
pub struct IrrbbBucket {
    pub label: &'static str,
    pub upper_years: f64,
    pub midpoint: f64,
}

/// The 19 IRRBB buckets (BB Table 3).
pub const IRRBB_BUCKETS: [IrrbbBucket; 19] = [
    IrrbbBucket { label: "O/N", upper_years: 1.0 / 365.0, midpoint: 0.0028 },
    IrrbbBucket { label: "O/N\u{2013}1M", upper_years: 1.0 / 12.0, midpoint: 0.0417 },
    IrrbbBucket { label: "1\u{2013}3M", upper_years: 3.0 / 12.0, midpoint: 0.1667 },
    IrrbbBucket { label: "3\u{2013}6M", upper_years: 6.0 / 12.0, midpoint: 0.375 },
    IrrbbBucket { label: "6\u{2013}9M", upper_years: 9.0 / 12.0, midpoint: 0.625 },
    IrrbbBucket { label: "9M\u{2013}1Y", upper_years: 1.0, midpoint: 0.875 },
    IrrbbBucket { label: "1\u{2013}1.5Y", upper_years: 1.5, midpoint: 1.25 },
    IrrbbBucket { label: "1.5\u{2013}2Y", upper_years: 2.0, midpoint: 1.75 },
    IrrbbBucket { label: "2\u{2013}3Y", upper_years: 3.0, midpoint: 2.5 },
    IrrbbBucket { label: "3\u{2013}4Y", upper_years: 4.0, midpoint: 3.5 },
    IrrbbBucket { label: "4\u{2013}5Y", upper_years: 5.0, midpoint: 4.5 },
    IrrbbBucket { label: "5\u{2013}6Y", upper_years: 6.0, midpoint: 5.5 },
    IrrbbBucket { label: "6\u{2013}7Y", upper_years: 7.0, midpoint: 6.5 },
    IrrbbBucket { label: "7\u{2013}8Y", upper_years: 8.0, midpoint: 7.5 },
    IrrbbBucket { label: "8\u{2013}9Y", upper_years: 9.0, midpoint: 8.5 },
    IrrbbBucket { label: "9\u{2013}10Y", upper_years: 10.0, midpoint: 9.5 },
    IrrbbBucket { label: "10\u{2013}15Y", upper_years: 15.0, midpoint: 12.5 },
    IrrbbBucket { label: "15\u{2013}20Y", upper_years: 20.0, midpoint: 17.5 },
    IrrbbBucket { label: ">20Y", upper_years: f64::INFINITY, midpoint: 25.0 },
];

/// The bucket a cash flow at `t` years falls in: the first whose upper bound is not below `t`. A
/// flow at `t` <= 0 (or at a blank time) is in O/N.
pub fn bucket_index(t: f64) -> usize {
    if !(t > 0.0) {
        return 0;
    }
    IRRBB_BUCKETS.iter().position(|b| t <= b.upper_years + 1e-9).unwrap_or(IRRBB_BUCKETS.len() - 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Asset,
    Liability,
}

impl Side {
    pub fn sign(self) -> f64 {
        match self {
            Side::Asset => 1.0,
            Side::Liability => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum RateType {
    #[default]
    Fixed,
    Floating,
}

impl RateType {
    pub fn label(self) -> &'static str {
        match self {
            RateType::Fixed => "Fixed",
            RateType::Floating => "Floating",
        }
    }
    /// "Fixed" / "Floating" (and the short forms banks use); anything else is `None`.
    pub fn parse(text: &str) -> Option<RateType> {
        match text.trim().to_lowercase().as_str() {
            "fixed" | "fix" | "fx" => Some(RateType::Fixed),
            "floating" | "float" | "flt" | "fl" | "variable" | "var" => Some(RateType::Floating),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Repayment {
    #[default]
    Bullet,
    EqualInstalment,
    EqualPrincipal,
}

impl Repayment {
    pub const ALL: [Repayment; 3] = [Repayment::Bullet, Repayment::EqualInstalment, Repayment::EqualPrincipal];
    pub fn label(self) -> &'static str {
        match self {
            Repayment::Bullet => "Bullet",
            Repayment::EqualInstalment => "Equal instalment",
            Repayment::EqualPrincipal => "Equal principal",
        }
    }
    pub fn parse(text: &str) -> Option<Repayment> {
        match text.trim().to_lowercase().replace(['-', '_'], " ").split_whitespace().collect::<Vec<_>>().join(" ").as_str() {
            "bullet" | "lump sum" | "lumpsum" => Some(Repayment::Bullet),
            "equal instalment" | "equal instalments" | "equal installment" | "equal installments" | "emi" | "annuity" => Some(Repayment::EqualInstalment),
            "equal principal" | "equal principal repayment" => Some(Repayment::EqualPrincipal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Frequency {
    Monthly,
    Quarterly,
    HalfYearly,
    Annual,
    #[default]
    AtMaturity,
}

impl Frequency {
    pub const ALL: [Frequency; 5] = [Frequency::Monthly, Frequency::Quarterly, Frequency::HalfYearly, Frequency::Annual, Frequency::AtMaturity];
    pub fn label(self) -> &'static str {
        match self {
            Frequency::Monthly => "Monthly",
            Frequency::Quarterly => "Quarterly",
            Frequency::HalfYearly => "Half-yearly",
            Frequency::Annual => "Annual",
            Frequency::AtMaturity => "At maturity",
        }
    }
    /// Payments per year; `None` for "At maturity".
    pub fn per_year(self) -> Option<u32> {
        match self {
            Frequency::Monthly => Some(12),
            Frequency::Quarterly => Some(4),
            Frequency::HalfYearly => Some(2),
            Frequency::Annual => Some(1),
            Frequency::AtMaturity => None,
        }
    }
    pub fn parse(text: &str) -> Option<Frequency> {
        match text.trim().to_lowercase().replace(['-', '_'], " ").split_whitespace().collect::<Vec<_>>().join(" ").as_str() {
            "monthly" | "month" | "m" | "1m" => Some(Frequency::Monthly),
            "quarterly" | "quarter" | "q" | "3m" => Some(Frequency::Quarterly),
            "half yearly" | "halfyearly" | "semi annual" | "semiannual" | "semi annually" | "h" | "6m" => Some(Frequency::HalfYearly),
            "annual" | "annually" | "yearly" | "y" | "a" | "12m" => Some(Frequency::Annual),
            "at maturity" | "maturity" | "bullet" | "end" | "lump sum" => Some(Frequency::AtMaturity),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CashFlowMode {
    #[default]
    PrincipalAndInterest,
    PrincipalOnly,
}

impl CashFlowMode {
    pub fn label(self) -> &'static str {
        match self {
            CashFlowMode::PrincipalAndInterest => "Principal + interest",
            CashFlowMode::PrincipalOnly => "Principal only",
        }
    }
    pub fn with_interest(self) -> bool {
        self == CashFlowMode::PrincipalAndInterest
    }
}

/// The six IRRBB scenarios of BB §14.2 and the base case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Scenario {
    #[default]
    Base,
    ParallelUp,
    ParallelDown,
    ShortUp,
    ShortDown,
    Steepener,
    Flattener,
}

impl Scenario {
    /// The six shock scenarios, in the order of the guideline.
    pub const SIX: [Scenario; 6] = [Scenario::ParallelUp, Scenario::ParallelDown, Scenario::Steepener, Scenario::Flattener, Scenario::ShortUp, Scenario::ShortDown];
    pub const ALL: [Scenario; 7] = [Scenario::Base, Scenario::ParallelUp, Scenario::ParallelDown, Scenario::Steepener, Scenario::Flattener, Scenario::ShortUp, Scenario::ShortDown];
    pub fn label(self) -> &'static str {
        match self {
            Scenario::Base => "Base",
            Scenario::ParallelUp => "Parallel up",
            Scenario::ParallelDown => "Parallel down",
            Scenario::ShortUp => "Short up",
            Scenario::ShortDown => "Short down",
            Scenario::Steepener => "Steepener",
            Scenario::Flattener => "Flattener",
        }
    }
    /// The CPR multiplier γ (BB §15.22–15.28): 0.8 for parallel up, steepener and short up; 1.2 for
    /// the other shocks; 1 in the base case.
    pub fn cpr_multiplier(self) -> f64 {
        match self {
            Scenario::Base => 1.0,
            Scenario::ParallelUp | Scenario::Steepener | Scenario::ShortUp => 0.8,
            _ => 1.2,
        }
    }
    /// The TDRR multiplier u: 1.2 for parallel up, flattener and short up; 0.8 for the other
    /// shocks; 1 in the base case.
    pub fn tdrr_multiplier(self) -> f64 {
        match self {
            Scenario::Base => 1.0,
            Scenario::ParallelUp | Scenario::Flattener | Scenario::ShortUp => 1.2,
            _ => 0.8,
        }
    }
}

/// What the cash flows are calculated for.
#[derive(Clone, Debug)]
pub struct CfSettings {
    pub cutoff: NaiveDate,
    pub mode: CashFlowMode,
    pub scenario: Scenario,
    /// "Treat blank maturity as O/N": a row with no maturity date (other than a non-maturity
    /// deposit of the behaviour profile, which needs none) goes in O/N instead of stopping the run.
    pub blank_maturity_on: bool,
}

impl CfSettings {
    /// The usual settings: a blank maturity stops the run.
    pub fn new(cutoff: NaiveDate, mode: CashFlowMode, scenario: Scenario) -> CfSettings {
        CfSettings { cutoff, mode, scenario, blank_maturity_on: false }
    }
}

/// A row of a book, parsed.
#[derive(Clone, Debug)]
pub struct CfRow {
    pub side: Side,
    /// The outstanding principal, positive.
    pub principal: f64,
    /// The annual rate in percent.
    pub rate_pct: f64,
    pub rate_type: RateType,
    pub maturity: Option<NaiveDate>,
    pub reprice: Option<NaiveDate>,
    pub repayment: Repayment,
    pub frequency: Frequency,
    pub product: String,
    pub currency: String,
}

impl CfRow {
    /// A fixed-rate bullet row, for tests and simple callers.
    pub fn simple(side: Side, principal: f64, maturity: NaiveDate) -> CfRow {
        CfRow {
            side,
            principal,
            rate_pct: 0.0,
            rate_type: RateType::Fixed,
            maturity: Some(maturity),
            reprice: None,
            repayment: Repayment::Bullet,
            frequency: Frequency::AtMaturity,
            product: String::new(),
            currency: "BDT".to_string(),
        }
    }
}

/// One dated cash flow of a row, unsigned: the principal and the interest parts.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub date: NaiveDate,
    /// Years from the cutoff date (days ÷ 365); 0 or less goes to O/N.
    pub t: f64,
    pub principal: f64,
    pub interest: f64,
}

impl Flow {
    pub fn total(&self) -> f64 {
        self.principal + self.interest
    }
}

/// The cash flows of one row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowFlows {
    pub flows: Vec<Flow>,
    /// An asset whose maturity has passed: it has no flows; this is its principal.
    pub matured_asset: Option<f64>,
    /// A row with no maturity date that "Treat blank maturity as O/N" put in O/N: its principal.
    pub blank_maturity: Option<f64>,
    /// A floating row whose next repricing date is on or before the cutoff: its principal, put in
    /// O/N with no interest (the date is stale or the rate has already reset).
    pub past_reprice: Option<f64>,
}

/// True when `d` is the last day of its month.
pub fn is_month_end(d: NaiveDate) -> bool {
    d.succ_opt().is_none_or(|n| n.month() != d.month())
}

/// The last day of the month of `d`.
pub fn last_day_of_month(d: NaiveDate) -> NaiveDate {
    let first = d.with_day(1).unwrap_or(d);
    first.checked_add_months(Months::new(1)).and_then(|n| n.pred_opt()).unwrap_or(d)
}

fn years(cutoff: NaiveDate, d: NaiveDate) -> f64 {
    (d - cutoff).num_days() as f64 / 365.0
}

/// The instalment of an annuity of `p` over `n` periods at the periodic rate `i`.
pub fn annuity_payment(p: f64, i: f64, n: u32) -> f64 {
    if n == 0 {
        return p;
    }
    if i.abs() < 1e-15 {
        return p / n as f64;
    }
    p * i / (1.0 - (1.0 + i).powi(-(n as i32)))
}

/// Longest schedule the engine will build (a monthly loan of 100 years).
const MAX_PAYMENTS: u32 = 1200;

/// The dated cash flows of one row from the cutoff date.
///
/// * Payment dates run back from the maturity by the frequency, keeping those after the cutoff;
///   each period's interest is the outstanding × rate ÷ payments per year (a full period, also for the first).
/// * A floating row runs its scheduled payments (by its repayment type) up to its next repricing
///   date; on the earlier of that date and its maturity all the principal that is left and that
///   period's interest flow, and nothing comes after.
/// * A floating row whose repricing date is on or before the cutoff goes to O/N, principal only (counted in the ladder's note).
/// * A matured liability goes to O/N; a matured asset has no flows.
/// * With a behaviour product: a non-maturity deposit splits into the non-core part (O/N) and the
///   core part at its average maturity (principal only); a loan's CPR and a term deposit's TDRR use
///   the scenario's multipliers.
pub fn generate_flows(row: &CfRow, settings: &CfSettings, beh: Option<&BehaviourProduct>) -> Result<RowFlows> {
    let cutoff = settings.cutoff;
    let with_interest = settings.mode.with_interest();
    let p = row.principal;
    if !(p.is_finite() && p >= 0.0) {
        bail!("The outstanding principal must be a number that is not negative.");
    }
    let mut out = RowFlows::default();
    if p == 0.0 {
        return Ok(out);
    }

    // a non-maturity deposit: principal only, no contractual maturity
    if let (Side::Liability, Some(b)) = (row.side, beh.filter(|b| b.is_nmd())) {
        let core = (b.core_pct / 100.0).clamp(0.0, 1.0);
        let on = p * (1.0 - core);
        if on > 0.0 {
            out.flows.push(Flow { date: cutoff, t: 0.0, principal: on, interest: 0.0 });
        }
        if core > 0.0 {
            let days = (b.avg_maturity_years * 365.0).round() as i64;
            let date = cutoff + chrono::Duration::days(days);
            out.flows.push(Flow { date, t: b.avg_maturity_years, principal: p * core, interest: 0.0 });
        }
        return Ok(out);
    }

    let maturity = match row.maturity {
        Some(m) => m,
        None if settings.blank_maturity_on => {
            // the owner's option: no maturity date, so all of it is in O/N (principal only)
            out.flows.push(Flow { date: cutoff, t: 0.0, principal: p, interest: 0.0 });
            out.blank_maturity = Some(p);
            return Ok(out);
        }
        None => bail!("The row has no maturity date."),
    };
    if maturity <= cutoff {
        match row.side {
            Side::Liability => out.flows.push(Flow { date: cutoff, t: 0.0, principal: p, interest: 0.0 }),
            Side::Asset => out.matured_asset = Some(p),
        }
        return Ok(out);
    }
    let rate = if row.rate_pct.is_finite() {
        row.rate_pct / 100.0
    } else if with_interest {
        bail!("The rate is not a number.");
    } else {
        0.0
    };

    // term deposit early redemption: the expected part goes to O/N
    let mut remaining = p;
    if let Some(b) = beh.filter(|b| !b.is_nmd() && b.tdrr_pct > 0.0 && row.side == Side::Liability) {
        let tdrr = (b.tdrr_pct / 100.0 * settings.scenario.tdrr_multiplier()).clamp(0.0, 1.0);
        let redeemed = p * tdrr;
        if redeemed > 0.0 {
            out.flows.push(Flow { date: cutoff, t: 0.0, principal: redeemed, interest: 0.0 });
        }
        remaining = p - redeemed;
    }
    if remaining <= 0.0 {
        return Ok(out);
    }

    let per_year = row.frequency.per_year();

    // a floating row whose repricing date has passed: all of it in O/N, principal only
    if row.rate_type == RateType::Floating && row.reprice.is_some_and(|r| r <= cutoff) {
        out.flows.push(Flow { date: cutoff, t: 0.0, principal: remaining, interest: 0.0 });
        out.past_reprice = Some(remaining);
        return Ok(out);
    }

    // a floating row: the scheduled payments before its next repricing date flow on their own
    // dates; on the repricing date all the remaining principal and that period's interest flow
    let reprice_on = if row.rate_type == RateType::Floating {
        let reprice = row.reprice.ok_or_else(|| anyhow!("A floating row has no repricing date."))?;
        Some(reprice.min(maturity))
    } else {
        None
    };

    // payment dates back from the maturity
    let dates: Vec<NaiveDate> = match row.frequency.per_year() {
        None => vec![maturity],
        Some(f) => {
            let step = 12 / f;
            // each date comes from the maturity, never from the date before it: a maturity on the last day of
            // its month gives month ends; otherwise the maturity's day, cut back to the month end where the
            // month is shorter
            let month_end = is_month_end(maturity);
            let mut v = Vec::new();
            let mut k: u32 = 0;
            loop {
                let mut d = maturity.checked_sub_months(Months::new(k * step)).ok_or_else(|| anyhow!("The payment dates of the row run out of the calendar."))?;
                if month_end {
                    d = last_day_of_month(d);
                }
                if d <= cutoff {
                    break;
                }
                v.push(d);
                k += 1;
                if k > MAX_PAYMENTS {
                    bail!("The maturity date {} is more than {} payments after the cutoff. Check the date.", maturity.format("%Y-%m-%d"), MAX_PAYMENTS);
                }
            }
            v.reverse();
            v
        }
    };
    let n = dates.len() as u32;
    if n == 0 {
        // cannot happen (the maturity is after the cutoff), but never lose the principal
        out.flows.push(Flow { date: maturity, t: years(cutoff, maturity), principal: remaining, interest: 0.0 });
        return Ok(out);
    }

    // prepayment: fixed-rate assets with a CPR (a single payment at maturity has no periods to prepay in)
    let prepay_q = match (beh.filter(|b| !b.is_nmd() && b.cpr_pct > 0.0 && row.side == Side::Asset && reprice_on.is_none()), per_year) {
        (Some(b), Some(f)) => {
            let cpr = (b.cpr_pct / 100.0 * settings.scenario.cpr_multiplier()).clamp(0.0, 1.0);
            1.0 - (1.0 - cpr).powf(1.0 / f as f64)
        }
        _ => 0.0,
    };

    let i = per_year.map(|f| rate / f as f64).unwrap_or(0.0);
    let mut bal = remaining;
    for (idx, date) in dates.iter().enumerate() {
        if reprice_on.is_some_and(|r| *date >= r) {
            break;
        }
        let n_rem = n - idx as u32;
        let t = years(cutoff, *date);
        let interest = if !with_interest {
            0.0
        } else if per_year.is_some() {
            bal * i
        } else {
            bal * rate * t
        };
        let prepay = (bal * prepay_q).min(bal);
        let after_prepay = bal - prepay;
        let sched = if n_rem == 1 {
            after_prepay
        } else {
            match row.repayment {
                Repayment::Bullet => 0.0,
                Repayment::EqualPrincipal => after_prepay / n_rem as f64,
                // an annuity over the remaining periods on the balance left after the prepayment: its principal
                // part is the instalment less the interest on that balance (the interest paid is on the opening balance)
                Repayment::EqualInstalment => (annuity_payment(after_prepay, i, n_rem) - after_prepay * i).clamp(0.0, after_prepay),
            }
        };
        let principal = prepay + sched;
        out.flows.push(Flow { date: *date, t, principal, interest });
        bal = (bal - principal).max(0.0);
    }
    if let Some(r) = reprice_on {
        // the repricing date (or the maturity, if that comes first): everything that is left
        if bal > 0.0 {
            let (date, t) = if r <= cutoff { (cutoff, 0.0) } else { (r, years(cutoff, r)) };
            let interest = if !with_interest {
                0.0
            } else if per_year.is_some() {
                bal * i
            } else {
                bal * rate * t.max(0.0)
            };
            out.flows.push(Flow { date, t, principal: bal, interest });
        }
    }
    Ok(out)
}

/// Assets and liabilities (negative) of the 19 buckets of one currency.
#[derive(Clone, Debug, PartialEq)]
pub struct BucketAmounts {
    pub assets: [f64; 19],
    /// Outflows, negative.
    pub liabilities: [f64; 19],
}

impl Default for BucketAmounts {
    fn default() -> Self {
        Self { assets: [0.0; 19], liabilities: [0.0; 19] }
    }
}

impl BucketAmounts {
    pub fn net(&self) -> [f64; 19] {
        let mut n = [0.0; 19];
        for (i, v) in n.iter_mut().enumerate() {
            *v = self.assets[i] + self.liabilities[i];
        }
        n
    }
}

/// The ladder of every currency and the assets that were left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LadderResult {
    /// One entry per currency: BDT first, then A–Z.
    pub currencies: Vec<(String, BucketAmounts)>,
    /// Matured assets that are not in the ladder: currency, count, total principal.
    pub excluded: Vec<(String, usize, f64)>,
    /// Rows with a blank maturity that "Treat blank maturity as O/N" put in O/N (they are in the
    /// O/N bucket already): currency, count, total of assets, total of liabilities (negative).
    pub blank_on: Vec<(String, usize, f64, f64)>,
    /// Floating rows whose repricing date was on or before the cutoff: put in O/N, principal only.
    pub past_reprice_rows: usize,
    /// Counts of rows treated with the behaviour profile, and other things worth saying.
    pub notes: Vec<String>,
}

impl LadderResult {
    pub fn for_currency(&self, currency: &str) -> Option<&BucketAmounts> {
        self.currencies.iter().find(|(c, _)| c == currency).map(|(_, b)| b)
    }
    pub fn excluded_count(&self) -> usize {
        self.excluded.iter().map(|e| e.1).sum()
    }
    pub fn excluded_total(&self) -> f64 {
        self.excluded.iter().map(|e| e.2).sum()
    }
    pub fn blank_on_count(&self) -> usize {
        self.blank_on.iter().map(|e| e.1).sum()
    }
}

fn currency_order(a: &str, b: &str) -> std::cmp::Ordering {
    match (a == "BDT", b == "BDT") {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// Slots every row's flows into the buckets.
pub fn ladder_from_rows(rows: &[CfRow], settings: &CfSettings, behaviour: Option<&BehaviourFile>) -> Result<LadderResult> {
    let mut res = LadderResult::default();
    let (mut nmd_rows, mut cpr_rows, mut tdrr_rows) = (0usize, 0usize, 0usize);
    for (n, row) in rows.iter().enumerate() {
        let beh = behaviour.and_then(|b| b.find(&row.product));
        match beh {
            Some(b) if b.is_nmd() && row.side == Side::Liability => nmd_rows += 1,
            Some(b) if b.cpr_pct > 0.0 && row.side == Side::Asset => cpr_rows += 1,
            Some(b) if b.tdrr_pct > 0.0 && row.side == Side::Liability => tdrr_rows += 1,
            _ => {}
        }
        let rf = generate_flows(row, settings, beh).with_context(|| format!("Row {} ({})", n + 1, row.product))?;
        let idx = match res.currencies.iter().position(|(c, _)| *c == row.currency) {
            Some(i) => i,
            None => {
                res.currencies.push((row.currency.clone(), BucketAmounts::default()));
                res.currencies.len() - 1
            }
        };
        let sign = row.side.sign();
        for f in &rf.flows {
            let b = bucket_index(f.t);
            let amt = f.total();
            match row.side {
                Side::Asset => res.currencies[idx].1.assets[b] += sign * amt,
                Side::Liability => res.currencies[idx].1.liabilities[b] += sign * amt,
            }
        }
        if let Some(p) = rf.blank_maturity {
            let (a, l) = match row.side {
                Side::Asset => (p, 0.0),
                Side::Liability => (0.0, -p),
            };
            match res.blank_on.iter_mut().find(|e| e.0 == row.currency) {
                Some(e) => {
                    e.1 += 1;
                    e.2 += a;
                    e.3 += l;
                }
                None => res.blank_on.push((row.currency.clone(), 1, a, l)),
            }
        }
        if rf.past_reprice.is_some() {
            res.past_reprice_rows += 1;
        }
        if let Some(p) = rf.matured_asset {
            match res.excluded.iter_mut().find(|e| e.0 == row.currency) {
                Some(e) => {
                    e.1 += 1;
                    e.2 += p;
                }
                None => res.excluded.push((row.currency.clone(), 1, p)),
            }
        }
    }
    res.currencies.sort_by(|a, b| currency_order(&a.0, &b.0));
    res.excluded.sort_by(|a, b| currency_order(&a.0, &b.0));
    res.blank_on.sort_by(|a, b| currency_order(&a.0, &b.0));
    if behaviour.is_some() {
        res.notes.push(format!("Behaviour profile applied: {} non-maturity deposit row(s), {} loan row(s) with a prepayment rate, {} term deposit row(s) with an early-redemption rate.", nmd_rows, cpr_rows, tdrr_rows));
    }
    if res.blank_on_count() > 0 {
        res.notes.push(format!("{} row(s) with a blank maturity date were put in O/N (\"Treat blank maturity as O/N\"), principal only.", res.blank_on_count()));
    }
    if res.past_reprice_rows > 0 {
        res.notes.push(format!("{} floating row(s) had a repricing date on or before the cutoff and were put in O/N, principal only.", res.past_reprice_rows));
    }
    if res.excluded_count() > 0 {
        res.notes.push(format!("{} matured asset row(s) with a total of {:.2} are not in the ladder (as in the liquidity gap ladder). Matured liabilities are in O/N.", res.excluded_count(), res.excluded_total()));
    }
    Ok(res)
}

// ------------------------------------------------------------------------------------------ tabs

/// What a tab holds and where each role is. Optional roles are `None`; their defaults are used.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct CfInputSpec {
    /// Asset, liability or a side column.
    pub role: crate::banking::GapRole,
    pub principal_col: String,
    pub rate_col: Option<String>,
    pub rate_type_col: Option<String>,
    pub default_rate_type: RateType,
    pub maturity_col: String,
    pub reprice_col: Option<String>,
    pub repay_col: Option<String>,
    pub default_repayment: Repayment,
    pub freq_col: Option<String>,
    pub default_frequency: Frequency,
    pub product_col: Option<String>,
    pub currency_col: Option<String>,
    pub default_currency: String,
}

fn given(c: &Option<String>) -> Option<&str> {
    c.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// What went wrong on some rows: the message, how many rows and the first 5 keys.
struct Issue {
    what: String,
    count: usize,
    first: Vec<String>,
}

#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    fn add(&mut self, what: &str, key: impl FnOnce() -> String) {
        let slot = match self.0.iter().position(|i| i.what == what) {
            Some(p) => p,
            None => {
                self.0.push(Issue { what: what.to_string(), count: 0, first: Vec::new() });
                self.0.len() - 1
            }
        };
        let i = &mut self.0[slot];
        i.count += 1;
        if i.first.len() < 5 {
            i.first.push(key());
        }
    }

    fn into_result(self, key_label: &str) -> Result<()> {
        self.into_result_as("Cash flows", key_label)
    }

    fn into_result_as(self, prefix: &str, key_label: &str) -> Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|i| format!("{} on {} row(s); the first {} {}: {}.", i.what, i.count, i.first.len(), key_label, i.first.join(", ")))
            .collect();
        bail!("{}: {}", prefix, parts.join(" "));
    }
}

/// The rows of a tab, parsed, with the key of each (its account number, or "row N") for messages.
#[derive(Clone, Debug, Default)]
pub struct ParsedRows {
    pub rows: Vec<CfRow>,
    pub keys: Vec<String>,
    /// "values of Account_No" or "rows".
    pub key_label: String,
}

/// The rows of a tab, parsed. Blank or unparseable required values stop the step with the count
/// and the first 5 account numbers (or row numbers).
pub fn rows_from_lazy(lf: LazyFrame, spec: &CfInputSpec, settings: &CfSettings, behaviour_on: bool) -> Result<ParsedRows> {
    let mut lf = lf;
    let schema = lf.collect_schema().context("Could not read the columns of a tab for the cash flows")?;
    let names: Vec<String> = schema.iter_names().map(|n| n.to_string()).collect();
    let need_col = |c: &str, what: &str| -> Result<()> {
        if schema.get(c).is_none() {
            bail!("Cash flows: a tab has no column '{}'. Choose its {} column again.", c, what);
        }
        Ok(())
    };
    need_col(&spec.principal_col, "outstanding principal")?;
    need_col(&spec.maturity_col, "maturity date")?;
    let dt = schema.get(spec.principal_col.as_str()).cloned().unwrap_or(DataType::Null);
    if !dt.is_numeric() {
        bail!("Cash flows: the outstanding column '{}' is {}, not a number. Set it to a number in the schema first.", spec.principal_col, dt);
    }
    let with_interest = settings.mode.with_interest();
    if with_interest && given(&spec.rate_col).is_none() {
        bail!("Cash flows: choose the rate column, or switch the cash-flow mode to \"Principal only\".");
    }
    for (c, what) in [(&spec.rate_col, "rate"), (&spec.rate_type_col, "rate type"), (&spec.reprice_col, "next repricing date"), (&spec.repay_col, "repayment type"), (&spec.freq_col, "frequency"), (&spec.product_col, "product"), (&spec.currency_col, "currency")] {
        if let Some(c) = given(c) {
            need_col(c, what)?;
        }
    }
    if behaviour_on && given(&spec.product_col).is_none() {
        bail!("Cash flows: choose the product column, which the behaviour profile is read against.");
    }
    let lf = crate::banking::ensure_date(lf, &spec.maturity_col)?;
    let lf = match given(&spec.reprice_col) {
        Some(r) => crate::banking::ensure_date(lf, r)?,
        None => lf,
    };

    // the columns as text or numbers, under fixed names
    let mut cols: Vec<Expr> = vec![
        col(spec.principal_col.as_str()).cast(DataType::Float64).alias("__p"),
        col(spec.maturity_col.as_str()).cast(DataType::Int32).alias("__mat"),
    ];
    let text = |c: &str, a: &str| col(c).cast(DataType::String).alias(a);
    match &spec.role {
        crate::banking::GapRole::Side(c) => {
            need_col(c, "side")?;
            cols.push(text(c, "__side"));
        }
        _ => {}
    }
    if let Some(c) = given(&spec.rate_col) {
        cols.push(col(c).cast(DataType::Float64).alias("__rate"));
    }
    if let Some(c) = given(&spec.rate_type_col) {
        cols.push(text(c, "__rtype"));
    }
    if let Some(c) = given(&spec.reprice_col) {
        cols.push(col(c).cast(DataType::Int32).alias("__rep"));
    }
    if let Some(c) = given(&spec.repay_col) {
        cols.push(text(c, "__repay"));
    }
    if let Some(c) = given(&spec.freq_col) {
        cols.push(text(c, "__freq"));
    }
    if let Some(c) = given(&spec.product_col) {
        cols.push(text(c, "__prod"));
    }
    if let Some(c) = given(&spec.currency_col) {
        cols.push(text(c, "__cur"));
    }
    let acct = crate::banking::guess_account_column(&names);
    if let Some(a) = &acct {
        cols.push(text(a, "__acct"));
    }
    let df = lf.select(cols).collect().context("Could not read the rows of a tab for the cash flows")?;

    let f64s = |n: &str| -> Option<Vec<Option<f64>>> { df.column(n).ok().and_then(|c| c.f64().ok()).map(|c| c.into_iter().collect()) };
    let strs = |n: &str| -> Option<Vec<Option<String>>> { df.column(n).ok().and_then(|c| c.str().ok()).map(|c| c.into_iter().map(|v| v.map(str::to_string)).collect()) };
    let days = |n: &str| -> Option<Vec<Option<i32>>> { df.column(n).ok().and_then(|c| c.i32().ok()).map(|c| c.into_iter().collect()) };
    let (p, mat) = (f64s("__p").unwrap_or_default(), days("__mat").unwrap_or_default());
    let (side, rate, rtype, rep, repay, freq, prod, cur, acc) = (strs("__side"), f64s("__rate"), strs("__rtype"), days("__rep"), strs("__repay"), strs("__freq"), strs("__prod"), strs("__cur"), strs("__acct"));
    let key = |i: usize| -> String {
        match acc.as_ref().and_then(|a| a.get(i)).and_then(|v| v.as_deref()).map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => v.to_string(),
            None => format!("row {}", i + 1),
        }
    };
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(|| anyhow!("calendar"))?;
    let to_date = |d: i32| chrono::TimeDelta::try_days(d as i64).and_then(|t| epoch.checked_add_signed(t));
    let default_cur = spec.default_currency.trim().to_uppercase();

    let mut issues = Issues::default();
    let mut rows = Vec::with_capacity(df.height());
    for i in 0..df.height() {
        // side
        let side_v = match &spec.role {
            crate::banking::GapRole::Asset => Some(Side::Asset),
            crate::banking::GapRole::Liability => Some(Side::Liability),
            crate::banking::GapRole::Side(_) => {
                let t = side.as_ref().and_then(|s| s.get(i)).and_then(|v| v.as_deref()).unwrap_or("").trim().to_uppercase();
                if t.starts_with('A') {
                    Some(Side::Asset)
                } else if t.starts_with('L') || t == "DEPOSIT" || t == "BORROWING" {
                    Some(Side::Liability)
                } else {
                    None
                }
            }
        };
        let Some(side_v) = side_v else {
            issues.add("the side is blank or not A / L", || key(i));
            continue;
        };
        // principal
        let principal = p.get(i).copied().flatten();
        let Some(principal) = principal.filter(|v| v.is_finite()) else {
            issues.add(&format!("the outstanding amount in '{}' is blank or not a number", spec.principal_col), || key(i));
            continue;
        };
        if principal < 0.0 {
            issues.add(&format!("the outstanding amount in '{}' is negative (use positive amounts; the side decides the sign)", spec.principal_col), || key(i));
            continue;
        }
        let product = prod.as_ref().and_then(|v| v.get(i)).and_then(|v| v.as_deref()).unwrap_or("").trim().to_string();
        // rate type
        let rate_type = match rtype.as_ref().map(|v| v.get(i).cloned().flatten().unwrap_or_default()) {
            None => spec.default_rate_type,
            Some(t) => match RateType::parse(&t) {
                Some(r) => r,
                None => {
                    issues.add(&format!("the rate type '{}' is not Fixed or Floating", t.trim()), || key(i));
                    continue;
                }
            },
        };
        let repayment = match repay.as_ref().map(|v| v.get(i).cloned().flatten().unwrap_or_default()) {
            None => spec.default_repayment,
            Some(t) => match Repayment::parse(&t) {
                Some(r) => r,
                None => {
                    issues.add(&format!("the repayment type '{}' is not Bullet, Equal instalment or Equal principal", t.trim()), || key(i));
                    continue;
                }
            },
        };
        let frequency = match freq.as_ref().map(|v| v.get(i).cloned().flatten().unwrap_or_default()) {
            None => spec.default_frequency,
            Some(t) => match Frequency::parse(&t) {
                Some(r) => r,
                None => {
                    issues.add(&format!("the frequency '{}' is not Monthly, Quarterly, Half-yearly, Annual or At maturity", t.trim()), || key(i));
                    continue;
                }
            },
        };
        let currency = match cur.as_ref().map(|v| v.get(i).cloned().flatten().unwrap_or_default()) {
            None => default_cur.clone(),
            Some(t) if t.trim().is_empty() => {
                issues.add(&format!("the currency in '{}' is blank", spec.currency_col.clone().unwrap_or_default()), || key(i));
                continue;
            }
            Some(t) => t.trim().to_uppercase(),
        };
        let maturity = mat.get(i).copied().flatten().and_then(to_date);
        let reprice = rep.as_ref().and_then(|v| v.get(i).copied().flatten()).and_then(to_date);
        let rate_v = match &rate {
            Some(r) => r.get(i).copied().flatten().filter(|v| v.is_finite()),
            None => Some(0.0),
        };
        // a blank maturity, rate or repricing date is checked later, once the behaviour profile
        // has said which rows are non-maturity deposits (they need no maturity date)
        rows.push((i, CfRow { side: side_v, principal, rate_pct: rate_v.unwrap_or(f64::NAN), rate_type, maturity, reprice, repayment, frequency, product, currency }));
    }
    let key_label = match &acct {
        Some(a) => format!("values of {}", a),
        None => "rows".to_string(),
    };
    issues.into_result(&key_label)?;
    let keys: Vec<String> = rows.iter().map(|(i, _)| key(*i)).collect();
    Ok(ParsedRows { rows: rows.into_iter().map(|(_, r)| r).collect(), keys, key_label })
}

/// Checks the values the cash flows need once it is known which rows the behaviour profile
/// handles: a non-maturity deposit needs no maturity date; every other row needs one, a rate (when
/// interest counts) and, if floating, a repricing date. Stops with the count and the first 5 keys.
pub fn check_rows(parsed: &ParsedRows, settings: &CfSettings, behaviour: Option<&BehaviourFile>) -> Result<()> {
    let key = |i: usize| parsed.keys.get(i).cloned().unwrap_or_else(|| format!("row {}", i + 1));
    let mut issues = Issues::default();
    for (i, r) in parsed.rows.iter().enumerate() {
        let nmd = r.side == Side::Liability && behaviour.and_then(|b| b.find(&r.product)).is_some_and(|b| b.is_nmd());
        if nmd {
            continue;
        }
        if r.maturity.is_none() {
            if !settings.blank_maturity_on {
                issues.add("the maturity date is blank", || key(i));
            }
            continue;
        }
        if r.maturity.is_some_and(|m| m <= settings.cutoff) {
            continue;
        }
        if settings.mode.with_interest() && !r.rate_pct.is_finite() {
            issues.add("the rate is blank or not a number", || key(i));
        }
        if r.rate_type == RateType::Floating && r.reprice.is_none() {
            issues.add("a floating row has no next repricing date", || key(i));
        }
    }
    let blank = issues.0.iter().any(|i| i.what == "the maturity date is blank");
    let res = issues.into_result(&parsed.key_label);
    if blank {
        return res.map_err(|e| anyhow!("{} Only a product that is a non-maturity deposit in the behaviour profile needs no maturity date. Fill the dates, or tick \"Treat blank maturity as O/N\" to put those rows in O/N.", e));
    }
    res
}

/// The ladder of several tabs, with the optional behaviour profile.
pub fn build_ladder(inputs: Vec<(LazyFrame, CfInputSpec)>, settings: &CfSettings, behaviour: Option<&BehaviourFile>) -> Result<LadderResult> {
    if inputs.is_empty() {
        bail!("Cash flows: add at least one tab (assets or liabilities).");
    }
    let mut all: Vec<CfRow> = Vec::new();
    for (lf, spec) in inputs {
        let parsed = rows_from_lazy(lf, &spec, settings, behaviour.is_some())?;
        check_rows(&parsed, settings, behaviour)?;
        all.extend(parsed.rows);
    }
    ladder_from_rows(&all, settings, behaviour)
}

/// The rows of several tabs for the IRRBB run: parsed and checked like the ladder's. With
/// `rate_needed` (a floor on current rates for ΔNII) every row also needs a current rate.
pub fn build_rows_for_irrbb(inputs: Vec<(LazyFrame, CfInputSpec)>, settings: &CfSettings, behaviour: Option<&BehaviourFile>, rate_needed: bool) -> Result<Vec<CfRow>> {
    if inputs.is_empty() {
        bail!("IRRBB: add at least one tab (assets or liabilities).");
    }
    let mut all: Vec<CfRow> = Vec::new();
    for (lf, spec) in inputs {
        let parsed = rows_from_lazy(lf, &spec, settings, behaviour.is_some())?;
        check_rows(&parsed, settings, behaviour)?;
        if rate_needed {
            if given(&spec.rate_col).is_none() {
                bail!("IRRBB: choose the rate column: a floor on current rates is set, and \u{394}NII needs each row's current rate.");
            }
            let mut issues = Issues::default();
            for (i, r) in parsed.rows.iter().enumerate() {
                if !r.rate_pct.is_finite() {
                    issues.add("the current rate is blank, and a floor is set, so the shocked rate cannot be checked against it", || parsed.keys.get(i).cloned().unwrap_or_else(|| format!("row {}", i + 1)));
                }
            }
            issues.into_result_as("\u{394}NII", &parsed.key_label)?;
        }
        all.extend(parsed.rows);
    }
    Ok(all)
}

// ----------------------------------------------------------------------------------------- output

pub const LADDER_COLUMNS: [&str; 7] = ["Currency", "Bucket", "Midpoint (years)", "Assets", "Liabilities", "Net", "Cumulative net"];

/// The label of the row below a currency's buckets with the matured assets that are not in the ladder.
pub fn excluded_label(count: usize) -> String {
    format!("Matured assets, not in ladder ({})", count)
}

/// The label of the row below a currency's buckets that says how many rows with a blank maturity
/// "Treat blank maturity as O/N" put in O/N (their principal is in the O/N bucket already).
pub fn blank_on_label(count: usize) -> String {
    format!("Blank maturity, in O/N above ({})", count)
}

/// The ladder as a table: one row per bucket and currency (liabilities negative), then a row per
/// currency with the rows of a blank maturity that were put in O/N (if any), then a row per
/// currency with the matured assets that were left out.
pub fn ladder_frame(res: &LadderResult) -> Result<DataFrame> {
    let (mut cur, mut bucket, mut mid, mut a, mut l, mut n, mut c): (Vec<String>, Vec<String>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>) = Default::default();
    for (ccy, amounts) in &res.currencies {
        let net = amounts.net();
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
        }
    }
    for (ccy, count, assets, liabilities) in &res.blank_on {
        cur.push(ccy.clone());
        bucket.push(blank_on_label(*count));
        mid.push(None);
        a.push(Some(*assets));
        l.push(Some(*liabilities));
        n.push(None);
        c.push(None);
    }
    for (ccy, count, total) in &res.excluded {
        cur.push(ccy.clone());
        bucket.push(excluded_label(*count));
        mid.push(None);
        a.push(Some(*total));
        l.push(None);
        n.push(None);
        c.push(None);
    }
    Ok(DataFrame::new(vec![
        Series::new(LADDER_COLUMNS[0].into(), cur),
        Series::new(LADDER_COLUMNS[1].into(), bucket),
        Series::new(LADDER_COLUMNS[2].into(), mid),
        Series::new(LADDER_COLUMNS[3].into(), a),
        Series::new(LADDER_COLUMNS[4].into(), l),
        Series::new(LADDER_COLUMNS[5].into(), n),
        Series::new(LADDER_COLUMNS[6].into(), c),
    ])?)
}

/// `normalise_product` re-exported for callers that match product codes.
pub fn product_key(code: &str) -> String {
    normalise_product(code)
}
