//! Banking chart templates (Phase 7 C5): "Chart this". A template reads the table of a tool's result
//! tab (or of a tab with the right columns) and returns the chart the banker would draw by hand: a
//! `ChartSpec` and, where the chart needs numbers the tab does not have as columns (a ΔEVE as % of
//! Tier 1, the steps of a bridge), the small table to chart. The title is a finding computed from the
//! data by fixed rules (no AI); a takeaway the user types replaces it.
//!
//! Every function here is plain arithmetic on a `DataFrame`: the studio opens the derived table as a
//! tab, so the numbers behind the chart can be seen and exported.

use super::axis::{value_text, ValueKind, MINUS};
use super::spec::{BarStatus, ChartAgg, ChartKind, ChartScale, ChartSpec, LimitTick, MeasureSpec, PanelSpec, RefLine, Stack, Status, WaterfallMode};
use crate::models::format_with_commas;
use crate::sort_lists::SortOrder;
use anyhow::{anyhow, bail, Result};
use polars::prelude::*;

/// The thirteen templates, in the order of the gallery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Template {
    GapLadder,
    RepricingGap,
    NiiBridge,
    ClMigration,
    ClassifiedTrend,
    IrrbbScenarios,
    YieldCurves,
    LcrNsfr,
    EclByStage,
    DepositMix,
    RateProfile,
    BehaviourProfile,
    DataQuality,
}

impl Template {
    pub fn all() -> &'static [Template] {
        &[
            Template::GapLadder,
            Template::RepricingGap,
            Template::NiiBridge,
            Template::ClMigration,
            Template::ClassifiedTrend,
            Template::IrrbbScenarios,
            Template::YieldCurves,
            Template::LcrNsfr,
            Template::EclByStage,
            Template::DepositMix,
            Template::RateProfile,
            Template::BehaviourProfile,
            Template::DataQuality,
        ]
    }

    pub fn id(&self) -> &'static str {
        match self {
            Template::GapLadder => "gap-ladder",
            Template::RepricingGap => "repricing-gap",
            Template::NiiBridge => "nii-bridge",
            Template::ClMigration => "cl-migration",
            Template::ClassifiedTrend => "classified-trend",
            Template::IrrbbScenarios => "irrbb-scenarios",
            Template::YieldCurves => "yield-curves",
            Template::LcrNsfr => "lcr-nsfr",
            Template::EclByStage => "ecl-by-stage",
            Template::DepositMix => "deposit-mix",
            Template::RateProfile => "rate-profile",
            Template::BehaviourProfile => "behaviour-profile",
            Template::DataQuality => "data-quality",
        }
    }

    pub fn from_id(id: &str) -> Option<Template> {
        Template::all().iter().copied().find(|t| t.id() == id)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Template::GapLadder => "Gap ladder",
            Template::RepricingGap => "Repricing gap",
            Template::NiiBridge => "NII bridge",
            Template::ClMigration => "CL migration",
            Template::ClassifiedTrend => "Classified trend",
            Template::IrrbbScenarios => "IRRBB scenarios",
            Template::YieldCurves => "Yield curves",
            Template::LcrNsfr => "LCR / NSFR",
            Template::EclByStage => "ECL by stage",
            Template::DepositMix => "Deposit / loan mix",
            Template::RateProfile => "Rate profile",
            Template::BehaviourProfile => "Behaviour profile",
            Template::DataQuality => "Data quality",
        }
    }

    /// The tool whose result the template charts.
    pub fn source(&self) -> &'static str {
        match self {
            Template::GapLadder => "Liquidity gap ladder",
            Template::RepricingGap => "\u{394}NII repricing summary",
            Template::NiiBridge => "MoM variance",
            Template::ClMigration => "CL migration matrix",
            Template::ClassifiedTrend => "A tab of months by grade",
            Template::IrrbbScenarios => "IRRBB (six scenarios)",
            Template::YieldCurves => "Yield curves store",
            Template::LcrNsfr => "LCR / NSFR summary",
            Template::EclByStage => "IFRS 9 summary",
            Template::DepositMix => "Any tab with product and month",
            Template::RateProfile => "Rate profile",
            Template::BehaviourProfile => "A balance history tab",
            Template::DataQuality => "Data-quality scorecard",
        }
    }

    pub fn form(&self) -> &'static str {
        match self {
            Template::GapLadder => "Net gap columns over a cumulative gap % line, with limit ticks and ringed breaches",
            Template::RepricingGap => "Diverging gap columns with the cumulative gap as a linked panel",
            Template::NiiBridge => "Waterfall: prior interest, volume, rate, mix, current interest",
            Template::ClMigration => "Heatmap of the % of each previous grade's balance",
            Template::ClassifiedTrend => "Stacked columns by grade over months, with an NPL % panel",
            Template::IrrbbScenarios => "Horizontal diverging bars of \u{394}EVE % of Tier 1, with the outlier line",
            Template::YieldCurves => "Lines by curve date, the latest highlighted",
            Template::LcrNsfr => "Bars of each ratio with its minimum marked",
            Template::EclByStage => "EAD, ECL and coverage % by stage in linked panels (no dual axis)",
            Template::DepositMix => "Stacked columns over months",
            Template::RateProfile => "Columns of balance by rate band, in band order",
            Template::BehaviourProfile => "Balance-history line with the core level as a shaded band",
            Template::DataQuality => "Horizontal bars of pass % by rule, with the Green and Amber lines",
        }
    }

    /// One line for the gallery card: "use it when…".
    pub fn use_when(&self) -> &'static str {
        match self {
            Template::GapLadder => "you present the liquidity gap to ALCO against the policy limits.",
            Template::RepricingGap => "you show where repricing risk sits by bucket.",
            Template::NiiBridge => "you explain a month-on-month change in interest by volume, rate and mix.",
            Template::ClMigration => "you review upgrades and downgrades between grades.",
            Template::ClassifiedTrend => "you show how classified loans and the NPL ratio moved over months.",
            Template::IrrbbScenarios => "you compare the six IRRBB scenarios with the 15% outlier test.",
            Template::YieldCurves => "you compare the FTP curves of different dates.",
            Template::LcrNsfr => "you show the liquidity ratios against their minimums.",
            Template::EclByStage => "you show where the expected credit loss is held.",
            Template::DepositMix => "you show how a book's mix changed month by month.",
            Template::RateProfile => "you show how much of a book sits at each interest-rate band.",
            Template::BehaviourProfile => "you show a deposit's balance history against its core level.",
            Template::DataQuality => "you report which data-quality rules fall below the thresholds.",
        }
    }

    pub fn avoid_when(&self) -> &'static str {
        match self {
            Template::GapLadder => "the ladder has a single bucket: a table is clearer.",
            Template::RepricingGap => "you need the \u{394}NII figure: read it from the table.",
            Template::NiiBridge => "the interest of the two months is not in the MoM table.",
            Template::ClMigration => "you need account counts per cell: use the matrix itself.",
            Template::ClassifiedTrend => "the tab has not been pivoted to one row per month and grade.",
            Template::IrrbbScenarios => "you want the cash-flow ladder: that is a table.",
            Template::YieldCurves => "there is a single curve: a table of its points is enough.",
            Template::LcrNsfr => "you need the HQLA breakdown: read the lines table.",
            Template::EclByStage => "you need the grade-group split: use the summary table.",
            Template::DepositMix => "there is only one product: use a line.",
            Template::RateProfile => "the rate column has not been put into bands yet.",
            Template::BehaviourProfile => "there are fewer than 3 month-ends: the core level cannot be worked out.",
            Template::DataQuality => "no rule has a pass % (control totals only).",
        }
    }
}

/// How the numbers of a finding are written (the chart's own scale) and the thresholds a template reads.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// One unit of the chart's scale in BDT (10,000,000 for Crore; 1 for plain numbers).
    pub divisor: f64,
    /// "Cr", "L", "M", "K" or empty.
    pub suffix: String,
    pub decimals: usize,
    /// The data-quality thresholds of the rule set the scorecard was run with. There is no default: a regulatory
    /// or policy threshold lives in a data file, and a template that needs one without it stops with a message.
    pub dq_green: Option<f64>,
    pub dq_amber: Option<f64>,
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx { divisor: 1.0, suffix: String::new(), decimals: 2, dq_green: None, dq_amber: None }
    }
}

/// What a template makes.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub template: Template,
    /// The table to chart when the tab does not have it as it is (open it as a tab, then chart it).
    pub table: Option<DataFrame>,
    pub spec: ChartSpec,
    /// The title the chart gets: a finding computed from the data.
    pub finding: String,
    /// The name of the derived tab.
    pub tab_title: String,
    /// A second chart of the same result (the HQLA composition beside the LCR bars, the effects of
    /// each key beside the NII bridge); it opens as a tab of its own.
    pub companion: Option<Box<Prepared>>,
}

// ---------------------------------------------------------------------------------------- helpers

fn missing(t: Template, col: &str) -> anyhow::Error {
    anyhow!("{}: the tab has no column '{}'. Open the result of {} first.", t.name(), col, t.source())
}

fn has(df: &DataFrame, col: &str) -> bool {
    df.column(col).is_ok()
}

fn strs(t: Template, df: &DataFrame, col: &str) -> Result<Vec<Option<String>>> {
    let c = df.column(col).map_err(|_| missing(t, col))?;
    let s = c.cast(&DataType::String).map_err(|e| anyhow!("{}: the column '{}' could not be read as text: {}", t.name(), col, e))?;
    Ok(s.str()?.into_iter().map(|v| v.map(str::to_string)).collect())
}

fn nums(t: Template, df: &DataFrame, col: &str) -> Result<Vec<Option<f64>>> {
    let c = df.column(col).map_err(|_| missing(t, col))?;
    if !(c.dtype().is_numeric() || matches!(c.dtype(), DataType::Decimal(..))) {
        bail!("{}: the column '{}' is {}, not a number. Set it to a number in Schema & Ingestion first.", t.name(), col, c.dtype());
    }
    Ok(c.cast(&DataType::Float64)?.f64()?.into_iter().collect())
}

/// The text of the result's "Sample parameters" row (a parameter file marked sample fed the tool): it goes into the
/// footnote, so a chart made from sample numbers says so wherever it is shown. `None` when the result has no such row.
fn sample_text(df: &DataFrame, key_col: &str, text_col: &str) -> Option<String> {
    let key = df.column(key_col).ok()?.cast(&DataType::String).ok()?;
    let text = df.column(text_col).ok()?.cast(&DataType::String).ok()?;
    let i = key.str().ok()?.into_iter().position(|k| k == Some("Sample parameters"))?;
    text.str().ok()?.get(i).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// `footnote` with the sample warning after it (the warning comes first so it is not cut off).
fn with_sample(footnote: &str, sample: Option<String>) -> String {
    match sample {
        Some(s) if footnote.trim().is_empty() => s,
        Some(s) => format!("{} \u{b7} {}", s, footnote.trim()),
        None => footnote.to_string(),
    }
}

fn order_of(values: &[String]) -> SortOrder {
    SortOrder::List { name: "template".to_string(), values: values.to_vec(), case_insensitive: true }
}

/// "-10.5" as "−10.5": the title and axis minus.
fn minus(s: String) -> String {
    match s.strip_prefix('-') {
        Some(r) => format!("{}{}", MINUS, r),
        None => s,
    }
}

/// A percentage in a title: one decimal, a true minus sign: "−10.5%".
fn pct_title(v: f64) -> String {
    minus(format!("{}%", format_with_commas(v, 1)))
}

/// A limit in a label: no decimals when it is a whole number: "−10%".
fn limit_text(v: f64) -> String {
    let body = if (v - v.round()).abs() < 1e-9 { format_with_commas(v, 0) } else { format_with_commas(v, 1) };
    minus(format!("{}%", body))
}

/// A signed amount in the chart's scale: "+13.5 Cr".
fn amount_signed(v: f64, ctx: &Ctx) -> String {
    let x = v / ctx.divisor;
    let sign = if x >= 0.0 { "+" } else { "\u{2212}" };
    let unit = if ctx.suffix.is_empty() { String::new() } else { format!(" {}", ctx.suffix) };
    format!("{}{}{}", sign, format_with_commas(x.abs(), 1), unit)
}

fn base_spec(category: &str, kind: ChartKind) -> ChartSpec {
    ChartSpec { category: category.to_string(), kind, top_n: 60, ..Default::default() }
}

fn measure(col: &str, label: &str, pct: bool) -> MeasureSpec {
    MeasureSpec { column: Some(col.to_string()), agg: ChartAgg::Sum, label: label.to_string(), pct, ..Default::default() }
}

const GRADES: [&str; 5] = ["STD", "SMA", "SS", "DF", "BL"];

fn grade_rank(s: &str) -> Option<usize> {
    let u = s.trim().to_uppercase().replace('/', "");
    GRADES.iter().position(|g| *g == u)
}

fn frame(cols: Vec<Series>) -> Result<DataFrame> {
    Ok(DataFrame::new(cols)?)
}

// ------------------------------------------------------------------------------------- detection

/// The templates that fit a table: by its columns and, for the tabs the user shaped, by its values.
pub fn detect(df: &DataFrame) -> Vec<Template> {
    let mut out = Vec::new();
    for t in Template::all() {
        if fits(*t, df) {
            out.push(*t);
        }
    }
    out
}

pub fn fits(t: Template, df: &DataFrame) -> bool {
    match t {
        Template::GapLadder => ["Bucket", "Net gap", "Cumulative gap % of total assets"].iter().all(|c| has(df, c)),
        Template::RepricingGap => ["Repricing bucket", "Gap", "Cumulative gap"].iter().all(|c| has(df, c)),
        Template::NiiBridge => ["Prev annual interest", "Curr annual interest", "Volume effect", "Rate effect", "Mix effect"].iter().all(|c| has(df, c)),
        Template::ClMigration => has(df, "Previous grade") && has(df, "Closed/Repaid"),
        Template::IrrbbScenarios => ["Scenario", "EVE change", "Loss % of Tier 1"].iter().all(|c| has(df, c)),
        Template::LcrNsfr => ["Item", "Value", "Unit"].iter().all(|c| has(df, c)) && strs(t, df, "Item").map(|v| v.iter().flatten().any(|s| s == "LCR" || s == "NSFR")).unwrap_or(false),
        Template::EclByStage => ["Section", "Group", "EAD", "ECL"].iter().all(|c| has(df, c)) && strs(t, df, "Section").map(|v| v.iter().flatten().any(|s| s == "Stage")).unwrap_or(false),
        Template::DataQuality => ["Rule", "Pass %", "Rows failing"].iter().all(|c| has(df, c)),
        Template::RateProfile => rate_band_col(df).is_some() && amount_col(df).is_some(),
        Template::ClassifiedTrend => classified_cols(df).is_some(),
        Template::DepositMix => mix_cols(df).is_some(),
        Template::BehaviourProfile => history_cols(df).is_some(),
        Template::YieldCurves => false,
    }
}

fn is_num(df: &DataFrame, name: &str) -> bool {
    df.column(name).map(|c| c.dtype().is_numeric() || matches!(c.dtype(), DataType::Decimal(..))).unwrap_or(false)
}

/// The amount column: a number column called balance, outstanding or amount, else none (never a guess among many).
fn amount_col(df: &DataFrame) -> Option<String> {
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    for hint in ["balance", "outstanding", "amount", "ead"] {
        if let Some(n) = names.iter().find(|n| n.to_lowercase().contains(hint) && is_num(df, n)) {
            return Some(n.clone());
        }
    }
    None
}

fn rate_band_col(df: &DataFrame) -> Option<String> {
    for n in df.get_column_names() {
        let l = n.to_lowercase().replace('_', " ");
        if l.contains("rate band") || l.contains("rate profile") {
            return Some(n.to_string());
        }
    }
    None
}

fn month_col(df: &DataFrame) -> Option<String> {
    for n in df.get_column_names() {
        let l = n.to_lowercase();
        let c = df.column(n).ok()?;
        let dated = matches!(c.dtype(), DataType::Date | DataType::Datetime(_, _));
        if dated || l.contains("month") || l == "period" || l.contains("as of") || l.contains("asof") || l == "date" {
            return Some(n.to_string());
        }
    }
    None
}

fn product_col(df: &DataFrame) -> Option<String> {
    for n in df.get_column_names() {
        let l = n.to_lowercase();
        if (l.contains("product") || l.contains("scheme") || l == "type") && df.column(n).map(|c| matches!(c.dtype(), DataType::String)).unwrap_or(false) {
            return Some(n.to_string());
        }
    }
    None
}

/// (month, grade, balance) of a tab of months by classification grade.
fn classified_cols(df: &DataFrame) -> Option<(String, String, String)> {
    let month = month_col(df)?;
    let bal = amount_col(df)?;
    for n in df.get_column_names() {
        if *n == month.as_str() || *n == bal.as_str() {
            continue;
        }
        let Ok(c) = df.column(n) else { continue };
        let Ok(s) = c.cast(&DataType::String) else { continue };
        let vals: Vec<String> = s.str().ok()?.into_iter().flatten().map(str::to_string).collect();
        let mut distinct: Vec<&String> = vals.iter().collect();
        distinct.sort();
        distinct.dedup();
        if distinct.len() >= 2 && distinct.iter().all(|v| grade_rank(v).is_some()) {
            return Some((month, n.to_string(), bal));
        }
    }
    None
}

fn mix_cols(df: &DataFrame) -> Option<(String, String, String)> {
    Some((month_col(df)?, product_col(df)?, amount_col(df)?))
}

fn history_cols(df: &DataFrame) -> Option<(String, String)> {
    let date = df.get_column_names().iter().find(|n| df.column(n).map(|c| matches!(c.dtype(), DataType::Date)).unwrap_or(false)).map(|n| n.to_string())?;
    Some((date, amount_col(df)?))
}

// ------------------------------------------------------------------------------------------ prepare

pub fn prepare(t: Template, df: &DataFrame, ctx: &Ctx) -> Result<Prepared> {
    let mut p = prepare_inner(t, df, ctx)?;
    stamp(&mut p);
    Ok(p)
}

/// Marks the chart (and its second chart) with the template it came from: the subtitle of a template chart gets the unit.
fn stamp(p: &mut Prepared) {
    if p.spec.template.is_empty() {
        p.spec.template = p.template.name().to_string();
    }
    if let Some(c) = p.companion.as_mut() {
        stamp(c);
    }
}

fn prepare_inner(t: Template, df: &DataFrame, ctx: &Ctx) -> Result<Prepared> {
    match t {
        Template::GapLadder => gap_ladder(df),
        Template::RepricingGap => repricing_gap(df, ctx),
        Template::NiiBridge => nii_bridge(df, ctx),
        Template::ClMigration => cl_migration(df),
        Template::ClassifiedTrend => classified_trend(df),
        Template::IrrbbScenarios => irrbb(df, ctx),
        Template::LcrNsfr => lcr_nsfr(df),
        Template::EclByStage => ecl_by_stage(df),
        Template::DepositMix => deposit_mix(df),
        Template::RateProfile => rate_profile(df),
        Template::BehaviourProfile => behaviour(df),
        Template::DataQuality => data_quality(df, ctx),
        Template::YieldCurves => bail!("Yield curves are charted from the curve store: use Charts \u{25b8} New chart \u{25b8} Banking templates \u{25b8} Yield curves."),
    }
}

// ------------------------------------------------------------------------------------- gap ladder

fn gap_ladder(df: &DataFrame) -> Result<Prepared> {
    let t = Template::GapLadder;
    let bucket = strs(t, df, "Bucket")?;
    let net = nums(t, df, "Net gap")?;
    let cum = nums(t, df, "Cumulative gap % of total assets")?;
    let limit = if has(df, "Cumulative gap limit %") { nums(t, df, "Cumulative gap limit %")? } else { vec![None; df.height()] };
    let (mut b, mut n, mut c, mut l): (Vec<String>, Vec<f64>, Vec<f64>, Vec<Option<f64>>) = Default::default();
    for i in 0..df.height() {
        let (Some(name), Some(nv), Some(cv)) = (bucket[i].clone(), net[i], cum[i]) else { continue };
        // the totals row and the overdue row are not buckets
        if name == "Total" || name.starts_with("Overdue assets") {
            continue;
        }
        b.push(name);
        n.push(nv);
        c.push(cv);
        l.push(limit[i]);
    }
    if b.is_empty() {
        bail!("{}: the ladder has no bucket with a net gap and a cumulative gap %. Build the ladder with total assets above zero.", t.name());
    }
    let table = frame(vec![Series::new("Bucket".into(), b.clone()), Series::new("Net gap".into(), n), Series::new("Cumulative gap % of total assets".into(), c.clone())])?;
    let mut spec = base_spec("Bucket", ChartKind::Bar);
    spec.order = order_of(&b);
    spec.value = Some("Net gap".into());
    spec.measures = vec![measure("Net gap", "Net gap", false), measure("Cumulative gap % of total assets", "Cumulative gap, % of assets", true)];
    spec.panels = vec![
        PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: "Net gap".into(), single: false },
        PanelSpec { kind: ChartKind::Line, measures: vec![1], title: "Cumulative gap, % of assets".into(), single: false },
    ];
    spec.subtitle = "Liquidity gap ladder".to_string();
    let mut any_limit = false;
    for (i, name) in b.iter().enumerate() {
        let Some(lim) = l[i] else { continue };
        any_limit = true;
        // a cumulative gap equal to the limit is not a breach (as in the tool)
        let breach = if c[i] < lim { format!("Breach: {} vs {} limit", value_text(c[i], ValueKind::Percent, 1, false), limit_text(lim)) } else { String::new() };
        spec.limit_ticks.push(LimitTick { panel: 1, category: name.clone(), value: lim, status: Status::Critical, breach });
    }
    if any_limit {
        spec.limit_note = "red ticks: minimum (ALCO limit)".to_string();
        spec.footnote = "Limits from the ALCO policy file".to_string();
    }
    let finding = match c.iter().position(|v| *v > 0.0) {
        Some(0) => "Cumulative gap is positive from the first bucket".to_string(),
        Some(i) => format!("Cumulative gap turns positive after {}", b[i]),
        None => format!("Cumulative gap stays negative to {}", b[b.len() - 1]),
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: gap ladder".to_string(), companion: None })
}

// --------------------------------------------------------------------------------- repricing gap

fn repricing_gap(df: &DataFrame, _ctx: &Ctx) -> Result<Prepared> {
    let t = Template::RepricingGap;
    let bucket = strs(t, df, "Repricing bucket")?;
    let gap = nums(t, df, "Gap")?;
    let cum = nums(t, df, "Cumulative gap")?;
    let (mut b, mut g, mut c): (Vec<String>, Vec<f64>, Vec<f64>) = Default::default();
    for i in 0..df.height() {
        let (Some(name), Some(gv)) = (bucket[i].clone(), gap[i]) else { continue };
        if name == "Total" || name.trim().is_empty() {
            continue;
        }
        b.push(name);
        g.push(gv);
        c.push(cum[i].unwrap_or(0.0));
    }
    if b.is_empty() {
        bail!("{}: the summary has no repricing bucket with a gap. Build the \u{394}NII repricing summary first.", t.name());
    }
    let table = frame(vec![Series::new("Repricing bucket".into(), b.clone()), Series::new("Gap".into(), g.clone()), Series::new("Cumulative gap".into(), c)])?;
    let mut spec = base_spec("Repricing bucket", ChartKind::Bar);
    spec.order = order_of(&b);
    spec.value = Some("Gap".into());
    spec.measures = vec![measure("Gap", "Repricing gap", false), measure("Cumulative gap", "Cumulative gap", false)];
    spec.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: "Gap".into(), single: false }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: "Cumulative gap".into(), single: false }];
    spec.subtitle = "Repricing gap by bucket".to_string();
    let (mut best, mut at) = (0.0f64, 0usize);
    for (i, v) in g.iter().enumerate() {
        if v.abs() > best.abs() {
            best = *v;
            at = i;
        }
    }
    let finding = if best == 0.0 { "The repricing gap is zero in every bucket".to_string() } else { format!("Largest repricing gap in {}", b[at]) };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: repricing gap".to_string(), companion: None })
}

// ------------------------------------------------------------------------------------- NII bridge

fn nii_bridge(df: &DataFrame, ctx: &Ctx) -> Result<Prepared> {
    let t = Template::NiiBridge;
    let key = df.get_columns().first().map(|c| c.name().to_string());
    let keys = match &key {
        Some(k) => strs(t, df, k)?,
        None => vec![None; df.height()],
    };
    // a key in one month only (New, Closed) has blank effects: its whole change goes to "New / closed". A blank is never
    // read as 0, and a key with only some of its effects blank is a table the chart cannot trust.
    let (prev_i, curr_i) = (nums(t, df, "Prev annual interest")?, nums(t, df, "Curr annual interest")?);
    let (vol_v, rate_v, mix_v) = (nums(t, df, "Volume effect")?, nums(t, df, "Rate effect")?, nums(t, df, "Mix effect")?);
    let (mut prev, mut curr, mut vol, mut rate, mut mix, mut new_closed) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut any_new_closed = false;
    for i in 0..df.height() {
        let name = keys.get(i).and_then(|k| k.clone()).unwrap_or_default();
        if name == "Total" {
            continue;
        }
        let (Some(p), Some(c)) = (prev_i[i], curr_i[i]) else {
            bail!("{}: the key '{}' has no value in 'Prev annual interest' or 'Curr annual interest'. Run the month-on-month comparison again with a rate column.", t.name(), name);
        };
        prev += p;
        curr += c;
        match (vol_v[i], rate_v[i], mix_v[i]) {
            (Some(v), Some(r), Some(m)) => {
                vol += v;
                rate += r;
                mix += m;
            }
            (None, None, None) => {
                any_new_closed = true;
                new_closed += c - p;
            }
            _ => bail!("{}: the key '{}' has some of its volume, rate and mix effects blank and some not. Run the month-on-month comparison again.", t.name(), name),
        }
    }
    // one key in the tab: the bridge is that key's and the title names it; several keys: the bridge adds them up
    let distinct: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for k in keys.iter().flatten() {
            if k != "Total" && !v.contains(k) {
                v.push(k.clone());
            }
        }
        v
    };
    let whose = if distinct.len() == 1 { format!("{} interest", distinct[0]) } else { "total interest".to_string() };
    let mut labels: Vec<String> = ["Prior interest", "Volume", "Rate", "Mix"].map(String::from).to_vec();
    let mut amounts = vec![prev, vol, rate, mix];
    if any_new_closed {
        labels.push("New / closed".to_string());
        amounts.push(new_closed);
    }
    labels.push("Current interest".to_string());
    amounts.push(curr);
    let table = frame(vec![Series::new("Step".into(), labels.clone()), Series::new("Interest (BDT)".into(), amounts)])?;
    let mut spec = base_spec("Step", ChartKind::Waterfall);
    spec.order = order_of(&labels);
    spec.value = Some("Interest (BDT)".into());
    spec.waterfall = WaterfallMode::StartStepsEnd;
    spec.subtitle = "Annualised interest".to_string();
    spec.footnote = "Volume = \u{394}Balance \u{d7} prior rate \u{b7} Rate = prior balance \u{d7} \u{394}rate \u{b7} Mix = \u{394}Balance \u{d7} \u{394}rate".to_string();
    // the finding: the largest effect and its share of all the effects
    let effects = [("Volume", vol), ("Rate", rate), ("Mix", mix), ("New / closed", new_closed)];
    let total_abs: f64 = effects.iter().map(|e| e.1.abs()).sum();
    let (name, e) = effects.iter().copied().fold(("Volume", 0.0f64), |a, b| if b.1.abs() > a.1.abs() { b } else { a });
    let change = curr - prev;
    let what = if change >= 0.0 { "rise" } else { "fall" };
    let finding = if total_abs == 0.0 || change == 0.0 && e == 0.0 {
        "Interest is unchanged between the two months".to_string()
    } else {
        let share = e.abs() / total_abs;
        let (phrase, verb) = match name {
            "Volume" => (if e >= 0.0 { "Higher balances" } else { "Lower balances" }, "explain"),
            "Rate" => (if e >= 0.0 { "Higher rates" } else { "Lower rates" }, "explain"),
            "Mix" => ("The product mix", "explains"),
            _ => ("New and closed items", "explain"),
        };
        if share >= 0.5 {
            format!("{} {} most of the {} {} in {}", phrase, verb, amount_signed(change, ctx), what, whose)
        } else {
            format!("{} is the largest effect behind the {} {} in {}", name, amount_signed(change, ctx), what, whose)
        }
    };
    spec.title = finding.clone();
    // several keys: a second chart with each key's three effects (they add up to that key's change)
    let companion = if distinct.len() >= 2 { nii_by_key(df, &key.unwrap_or_default(), &keys, ctx)?.map(Box::new) } else { None };
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: NII bridge".to_string(), companion })
}

/// The effects of each key (volume, rate, mix) as stacked columns, the keys with the largest change first.
fn nii_by_key(df: &DataFrame, key_col: &str, keys: &[Option<String>], ctx: &Ctx) -> Result<Option<Prepared>> {
    let t = Template::NiiBridge;
    let (vol, rate, mix) = (nums(t, df, "Volume effect")?, nums(t, df, "Rate effect")?, nums(t, df, "Mix effect")?);
    let (prev_i, curr_i) = (nums(t, df, "Prev annual interest")?, nums(t, df, "Curr annual interest")?);
    // per key: its three effects, or (a key in one month only, blank effects) its whole change as "New / closed"
    let mut rows: Vec<(String, Option<(f64, f64, f64)>, f64)> = Vec::new();
    for i in 0..df.height() {
        let Some(k) = keys.get(i).cloned().flatten() else { continue };
        if k == "Total" {
            continue;
        }
        let (Some(p), Some(c)) = (prev_i[i], curr_i[i]) else {
            bail!("{}: the key '{}' has no value in 'Prev annual interest' or 'Curr annual interest'.", t.name(), k);
        };
        let eff = match (vol[i], rate[i], mix[i]) {
            (Some(v), Some(r), Some(m)) => Some((v, r, m)),
            (None, None, None) => None,
            _ => bail!("{}: the key '{}' has some of its volume, rate and mix effects blank and some not.", t.name(), k),
        };
        let change = c - p;
        match rows.iter_mut().find(|x| x.0 == k) {
            Some(x) => match (&mut x.1, eff) {
                (Some(a), Some(e)) => {
                    a.0 += e.0;
                    a.1 += e.1;
                    a.2 += e.2;
                }
                (None, None) => x.2 += change,
                _ => bail!("{}: the key '{}' appears twice, once with effects and once without.", t.name(), k),
            },
            None => rows.push((k, eff, change)),
        }
    }
    if rows.len() < 2 {
        return Ok(None);
    }
    let net = |r: &(String, Option<(f64, f64, f64)>, f64)| r.1.map(|e| e.0 + e.1 + e.2).unwrap_or(r.2);
    rows.sort_by(|a, b| net(b).abs().partial_cmp(&net(a).abs()).unwrap_or(std::cmp::Ordering::Equal));
    let total_keys = rows.len();
    rows.truncate(crate::charts::tokens::MAX_FACETS);
    let (mut k, mut e, mut v): (Vec<String>, Vec<String>, Vec<f64>) = Default::default();
    let mut any_new_closed = false;
    for (name, eff, change) in &rows {
        match eff {
            Some((vo, ra, mi)) => {
                for (label, x) in [("Volume", vo), ("Rate", ra), ("Mix", mi)] {
                    k.push(name.clone());
                    e.push(label.to_string());
                    v.push(*x);
                }
            }
            None => {
                any_new_closed = true;
                k.push(name.clone());
                e.push("New / closed".to_string());
                v.push(*change);
            }
        }
    }
    let order: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let head = if key_col.is_empty() { "Key".to_string() } else { key_col.to_string() };
    let table = frame(vec![Series::new(head.as_str().into(), k), Series::new("Effect".into(), e), Series::new("Change in interest (BDT)".into(), v)])?;
    let mut spec = base_spec(&head, ChartKind::Bar);
    spec.series = Some("Effect".into());
    spec.value = Some("Change in interest (BDT)".into());
    spec.stack = Stack::Stacked;
    spec.order = order_of(&order);
    spec.colour_map.assign_all(&["Volume", "Rate", "Mix"]);
    if any_new_closed {
        spec.colour_map.assign("New / closed");
    }
    spec.net_marker = true;
    spec.subtitle = "Change in annualised interest by effect \u{b7} the tick is the net change".to_string();
    if total_keys > rows.len() {
        spec.footnote = format!("The {} keys with the largest change, of {}", rows.len(), total_keys);
    }
    let top = &rows[0];
    let change = net(top);
    let finding = format!("{} has the largest change: {}", top.0, amount_signed(change, ctx));
    spec.title = finding.clone();
    Ok(Some(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: NII effects by key".to_string(), companion: None }))
}

// ---------------------------------------------------------------------------------- CL migration

fn cl_migration(df: &DataFrame) -> Result<Prepared> {
    let t = Template::ClMigration;
    let prev = strs(t, df, "Previous grade")?;
    let segment = if has(df, "Segment") { Some(strs(t, df, "Segment")?) } else { None };
    // the columns: the five grades and "Closed/Repaid" (not "Total")
    let mut cols: Vec<String> = GRADES.iter().filter(|g| has(df, g)).map(|g| g.to_string()).collect();
    if has(df, "Closed/Repaid") {
        cols.push("Closed/Repaid".to_string());
    }
    let mut grid: Vec<Vec<Option<f64>>> = Vec::new();
    for c in &cols {
        grid.push(nums(t, df, c)?);
    }
    let (mut from, mut to, mut bal): (Vec<String>, Vec<String>, Vec<f64>) = Default::default();
    let all_seg = "All segments";
    let seg_in_use = segment.as_ref().map(|s| s.iter().any(|v| v.as_deref() == Some(all_seg))).unwrap_or(false);
    for i in 0..df.height() {
        let Some(p) = prev[i].clone() else { continue };
        if grade_rank(&p).is_none() {
            continue; // New, the totals
        }
        if seg_in_use && segment.as_ref().and_then(|s| s[i].as_deref()) != Some(all_seg) {
            continue;
        }
        // a grade with no balance in any column has nothing to chart
        if grid.iter().all(|g| g[i].unwrap_or(0.0) == 0.0) {
            continue;
        }
        for (ci, c) in cols.iter().enumerate() {
            from.push(p.clone());
            to.push(c.clone());
            bal.push(grid[ci][i].unwrap_or(0.0));
        }
    }
    if from.is_empty() {
        bail!("{}: the matrix has no grade rows. Build the CL migration matrix first.", t.name());
    }
    let table = frame(vec![Series::new("Previous grade".into(), from.clone()), Series::new("Current grade".into(), to.clone()), Series::new("Balance".into(), bal.clone())])?;
    let mut spec = base_spec("Current grade", ChartKind::Heatmap);
    spec.series = Some("Previous grade".into());
    spec.value = Some("Balance".into());
    spec.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, label: "% of previous grade".into(), transform: super::spec::Transform::ShareOfSeries, pct: true, ..Default::default() }];
    let order: Vec<String> = cols.clone();
    spec.order = order_of(&order);
    spec.subtitle = "CL migration, % of each previous grade's balance \u{2192} current grade".to_string();
    spec.footnote = "Diagonal = no change".to_string();
    // the largest downgrade: a move to a worse grade, as a share of the previous grade's row total
    let mut row_total: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (f, b) in from.iter().zip(bal.iter()) {
        *row_total.entry(f.clone()).or_default() += *b;
    }
    let mut best: Option<(f64, f64, String, String)> = None;
    for ((f, tt), b) in from.iter().zip(to.iter()).zip(bal.iter()) {
        let (Some(fr), Some(tr)) = (grade_rank(f), grade_rank(tt)) else { continue };
        if tr <= fr || *b <= 0.0 {
            continue;
        }
        let total = row_total.get(f).copied().unwrap_or(0.0);
        if total <= 0.0 {
            continue;
        }
        let share = *b / total * 100.0;
        if best.as_ref().is_none_or(|x| share > x.0 || (share == x.0 && *b > x.1)) {
            best = Some((share, *b, f.clone(), tt.clone()));
        }
    }
    let finding = match best {
        Some((share, _, f, tt)) => format!("{}% of {} moved to {}", format_with_commas(share, 1), f, tt),
        None => "No loan moved to a worse grade".to_string(),
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: CL migration".to_string(), companion: None })
}

// ------------------------------------------------------------------------------ classified trend

/// A month cell as a date: "2026-10-31", "31/10/2026" (day first, as the CBS writes it) or "2026-10" (the month's first day).
fn parse_month_text(s: &str) -> Option<chrono::NaiveDate> {
    let s = s.trim();
    for f in ["%Y-%m-%d", "%d/%m/%Y"] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
    }
    let (y, m) = s.split_once('-')?;
    if y.len() == 4 && m.len() <= 2 {
        return chrono::NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, 1);
    }
    None
}

/// The month column as dates, so months sort as dates and never as text. A Date column is used as it is; a text
/// column must be day/month/year or year-month-day; anything else is refused naming the first value.
fn month_dates(t: Template, df: &DataFrame, col: &str) -> Result<Vec<Option<chrono::NaiveDate>>> {
    let c = df.column(col).map_err(|_| missing(t, col))?;
    let text = match c.dtype() {
        DataType::Date | DataType::Datetime(_, _) => c.cast(&DataType::Date)?.cast(&DataType::String)?,
        _ => c.cast(&DataType::String)?,
    };
    let mut out = Vec::with_capacity(df.height());
    for v in text.str()?.into_iter() {
        match v {
            None => out.push(None),
            Some(x) => match parse_month_text(x) {
                Some(d) => out.push(Some(d)),
                None => bail!("{}: the month '{}' in '{}' is not a date. Use a Date column, or text written day/month/year (31/10/2026) or year-month-day (2026-10-31), so the months can be put in order.", t.name(), x, col),
            },
        }
    }
    Ok(out)
}

/// The label of each month: "Oct 2026"; two different days of one month fall back to the whole date.
fn month_labels(dates: &[chrono::NaiveDate]) -> Vec<String> {
    let short: Vec<String> = dates.iter().map(|d| d.format("%b %Y").to_string()).collect();
    let mut seen = std::collections::HashSet::new();
    if short.iter().all(|s| seen.insert(s.clone())) {
        short
    } else {
        dates.iter().map(|d| d.format("%-d %b %Y").to_string()).collect()
    }
}

fn month_label(s: &str) -> String {
    // 2026-10-31 -> "Oct 2026"; any other text stays as it is
    match chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
        Ok(d) => d.format("%b %Y").to_string(),
        Err(_) => s.to_string(),
    }
}

fn classified_trend(df: &DataFrame) -> Result<Prepared> {
    let t = Template::ClassifiedTrend;
    let (month_c, grade_c, bal_c) = classified_cols(df).ok_or_else(|| anyhow!("{}: the tab needs a month column, a grade column (STD, SMA, SS, DF, BL) and a balance column. Pivot the stacked months by grade first.", t.name()))?;
    let months = month_dates(t, df, &month_c)?;
    let grades = strs(t, df, &grade_c)?;
    let bal = nums(t, df, &bal_c)?;
    // month -> grade balances and which grades the month has a row for
    let mut by: std::collections::BTreeMap<chrono::NaiveDate, ([f64; 5], [bool; 5])> = std::collections::BTreeMap::new();
    for i in 0..df.height() {
        let (Some(m), Some(g), Some(b)) = (months[i], grades[i].clone(), bal[i]) else { continue };
        let Some(r) = grade_rank(&g) else { continue };
        let e = by.entry(m).or_insert_with(|| ([0.0; 5], [false; 5]));
        e.0[r] += b;
        e.1[r] = true;
    }
    if by.is_empty() {
        bail!("{}: no row has a month, a grade and a balance.", t.name());
    }
    // the NPL ratio is over all loans: a month without its Standard (STD) row would overstate it
    if let Some((d, _)) = by.iter().find(|(_, (_, has))| !has[0]) {
        bail!("{}: the month {} has no STD row, so its NPL ratio cannot be worked out. Add the Standard (STD) balance for that month, or remove the month from the tab.", t.name(), d.format("%-d %b %Y"));
    }
    let label_of = month_labels(&by.keys().copied().collect::<Vec<_>>());
    let by: Vec<(String, [f64; 5])> = by.into_iter().zip(label_of).map(|((_, (v, _)), l)| (l, v)).collect();
    let (mut mm, mut gg, mut bb, mut np): (Vec<String>, Vec<String>, Vec<f64>, Vec<f64>) = Default::default();
    let mut ratios: Vec<(String, f64)> = Vec::new();
    for (label, v) in &by {
        let total: f64 = v.iter().sum();
        let npl = if total != 0.0 { (v[2] + v[3] + v[4]) / total * 100.0 } else { 0.0 };
        ratios.push((label.clone(), npl));
        for (r, g) in GRADES.iter().enumerate() {
            // Standard (STD) is not stacked: only SMA, SS, DF and BL, SMA always (so every month has a row)
            if r >= 1 && (v[r] != 0.0 || r == 1) {
                mm.push(label.clone());
                gg.push(g.to_string());
                bb.push(v[r]);
                np.push(npl);
            }
        }
    }
    let labels: Vec<String> = by.iter().map(|(l, _)| l.clone()).collect();
    let table = frame(vec![Series::new("Month".into(), mm), Series::new("Grade".into(), gg), Series::new("Balance".into(), bb), Series::new("NPL %".into(), np)])?;
    let mut spec = base_spec("Month", ChartKind::Bar);
    spec.series = Some("Grade".into());
    spec.value = Some("Balance".into());
    spec.stack = Stack::Stacked;
    spec.order = order_of(&labels);
    spec.measures = vec![measure("Balance", "Balance", false), MeasureSpec { column: Some("NPL %".into()), agg: ChartAgg::Average, label: "NPL %".into(), pct: true, ..Default::default() }];
    spec.panels = vec![PanelSpec { kind: ChartKind::Bar, measures: vec![0], title: "Balance by grade (SMA, SS, DF, BL)".into(), single: false }, PanelSpec { kind: ChartKind::Line, measures: vec![1], title: "NPL % (SS, DF and BL)".into(), single: true }];
    spec.colour_map.assign_all(&GRADES);
    spec.subtitle = "Standard (STD) not shown \u{b7} classified loans by grade, by month".to_string();
    let (first, last) = (ratios.first().map(|r| r.1).unwrap_or(0.0), ratios.last().map(|r| r.1).unwrap_or(0.0));
    let finding = if ratios.len() < 2 || (last - first).abs() < 0.05 {
        format!("NPL ratio is {}%", format_with_commas(last, 1))
    } else if last > first {
        format!("NPL ratio rose to {}%", format_with_commas(last, 1))
    } else {
        format!("NPL ratio fell to {}%", format_with_commas(last, 1))
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: classified trend".to_string(), companion: None })
}

// --------------------------------------------------------------------------------------- IRRBB

fn irrbb(df: &DataFrame, _ctx: &Ctx) -> Result<Prepared> {
    let t = Template::IrrbbScenarios;
    let sc = strs(t, df, "Scenario")?;
    let eve = nums(t, df, "EVE change")?;
    let note = if has(df, "Note") { strs(t, df, "Note")? } else { vec![None; df.height()] };
    let tier1 = sc
        .iter()
        .position(|s| s.as_deref() == Some("Tier 1 capital"))
        .and_then(|i| note[i].clone())
        .and_then(|n| n.replace(',', "").trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .ok_or_else(|| anyhow!("{}: the results have no 'Tier 1 capital' row. Run the six scenarios again.", t.name()))?;
    // the outlier limit is the rule file's (outlier_pct): the tool writes it in the row label "Outlier (max loss ≥ 15% of
    // Tier 1)". Without it the chart has no outlier line to draw and stops: no limit is assumed.
    let outlier = sc
        .iter()
        .flatten()
        .find(|s| s.starts_with("Outlier"))
        .and_then(|s| s.split('\u{2265}').nth(1))
        .and_then(|s| s.trim().split('%').next().map(str::to_string))
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| anyhow!("{}: the results do not state the outlier limit (the row 'Outlier (max loss \u{2265} x% of Tier 1)', from outlier_pct in the IRRBB shock rule file). Run the six scenarios again with a rule file that has outlier_pct; no limit is assumed.", t.name()))?;
    let mut rows: Vec<(String, f64)> = Vec::new();
    for i in 0..df.height() {
        let (Some(name), Some(v)) = (sc[i].clone(), eve[i]) else { continue };
        rows.push((name, v / tier1 * 100.0));
    }
    if rows.is_empty() {
        bail!("{}: the results have no scenario with an EVE change.", t.name());
    }
    rows.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let col = "\u{394}EVE % of Tier 1";
    let names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let table = frame(vec![Series::new("Scenario".into(), names.clone()), Series::new(col.into(), rows.iter().map(|r| r.1).collect::<Vec<_>>())])?;
    let mut spec = base_spec("Scenario", ChartKind::Bar);
    spec.horizontal = true;
    spec.order = order_of(&names);
    spec.value = Some(col.into());
    spec.measures = vec![measure(col, col, true)];
    spec.ref_lines = vec![RefLine { value: -outlier, label: format!("Outlier {}", limit_text(-outlier)), status: Status::Critical, panel: 0 }];
    spec.subtitle = format!("\u{394}EVE by BB scenario, % of Tier 1 capital \u{b7} outlier test at {}", limit_text(-outlier));
    if let Some(src) = sc.iter().position(|s| s.as_deref() == Some("Rule source")).and_then(|i| note[i].clone()).filter(|s| !s.trim().is_empty()) {
        spec.footnote = src;
    }
    spec.footnote = with_sample(&spec.footnote, sample_text(df, "Scenario", "Note"));
    let (worst_name, worst) = rows[0].clone();
    let finding = if worst < 0.0 {
        format!("{} is the worst case at {} of Tier 1", worst_name, pct_title(worst))
    } else {
        format!("No scenario loses value: the smallest gain is {} of Tier 1 ({})", pct_title(worst), worst_name)
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: \u{394}EVE by scenario".to_string(), companion: None })
}

// ------------------------------------------------------------------------------------ LCR / NSFR

fn lcr_nsfr(df: &DataFrame) -> Result<Prepared> {
    let t = Template::LcrNsfr;
    let item = strs(t, df, "Item")?;
    let value = nums(t, df, "Value")?;
    let get = |name: &str| item.iter().position(|s| s.as_deref() == Some(name)).and_then(|i| value[i]);
    let note = if has(df, "Note") { strs(t, df, "Note")? } else { vec![None; df.height()] };
    let note_of = |name: &str| item.iter().position(|s| s.as_deref() == Some(name)).and_then(|i| note[i].clone());
    let mut cats: Vec<String> = Vec::new();
    let mut vals: Vec<f64> = Vec::new();
    let mut mins: Vec<f64> = Vec::new();
    let mut breaches: Vec<bool> = Vec::new();
    for r in ["LCR", "NSFR"] {
        if let Some(v) = get(r) {
            // the minimum and whether it is met are the tool's own answer; a summary without them is not guessed at
            let Some(min) = get(&format!("{} minimum", r)) else {
                bail!("{}: the summary has no '{} minimum' row, so the ratio cannot be drawn against its minimum. Run the LCR / NSFR tool again.", t.name(), r);
            };
            let status = note_of(&format!("{} status", r)).unwrap_or_default();
            let breach = if status.starts_with("Below") {
                true
            } else if status.starts_with("Meets") {
                false
            } else {
                bail!("{}: the summary's '{} status' row says '{}', not 'Meets the minimum' or 'Below the minimum'. Run the LCR / NSFR tool again.", t.name(), r, status);
            };
            cats.push(r.to_string());
            vals.push(v);
            mins.push(min);
            breaches.push(breach);
        }
    }
    if cats.is_empty() {
        bail!("{}: neither the LCR nor the NSFR has a value (the net cash outflows or the required stable funding are zero).", t.name());
    }
    let table = frame(vec![Series::new("Ratio".into(), cats.clone()), Series::new("Ratio %".into(), vals.clone())])?;
    let mut spec = base_spec("Ratio", ChartKind::Bar);
    spec.horizontal = true;
    spec.order = order_of(&cats);
    spec.value = Some("Ratio %".into());
    spec.measures = vec![measure("Ratio %", "Ratio %", true)];
    spec.subtitle = "Liquidity ratios against their minimums".to_string();
    spec.limit_note = "red ticks: minimum ratio".to_string();
    let sample = sample_text(df, "Item", "Note");
    spec.footnote = with_sample("", sample.clone());
    for (i, c) in cats.iter().enumerate() {
        let breach = if breaches[i] { format!("Breach: {} vs {} minimum", value_text(vals[i], ValueKind::Percent, 1, false), limit_text(mins[i])) } else { String::new() };
        spec.limit_ticks.push(LimitTick { panel: 0, category: c.clone(), value: mins[i], status: Status::Critical, breach });
    }
    let finding = format!("{} {}% against a {}% minimum", cats[0], format_with_commas(vals[0], 1), format_with_commas(mins[0], 0));
    spec.title = finding.clone();
    let mut companion = hqla_composition(&item, &value, t)?;
    if let Some(c) = companion.as_mut() {
        c.spec.footnote = with_sample(&c.spec.footnote, sample);
    }
    let companion = companion.map(Box::new);
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: LCR and NSFR".to_string(), companion })
}

/// What the HQLA is made of: one tall stacked column of Level 1, 2A and 2B (after their factors), and
/// the part the caps leave out. Needs the Level 1/2A/2B rows of the LCR summary; without them (an older
/// summary) there is nothing to compose and no second chart.
fn hqla_composition(item: &[Option<String>], value: &[Option<f64>], t: Template) -> Result<Option<Prepared>> {
    let get = |name: &str| item.iter().position(|s| s.as_deref() == Some(name)).and_then(|i| value.get(i).copied().flatten());
    let (Some(l1), Some(l2a), Some(l2b)) = (get("Level 1 assets (after factors)"), get("Level 2A assets (after factors)"), get("Level 2B assets (after factors)")) else {
        return Ok(None);
    };
    let capped = get("Adjustment for the Level 2B cap").unwrap_or(0.0) + get("Adjustment for the Level 2 cap").unwrap_or(0.0);
    let gross = l1 + l2a + l2b;
    if gross <= 0.0 {
        return Ok(None);
    }
    let mut levels: Vec<String> = vec!["Level 1".into(), "Level 2A".into(), "Level 2B".into()];
    let mut amounts = vec![l1, l2a, l2b];
    if capped > 0.0 {
        levels.push("Not counted (caps)".into());
        amounts.push(capped);
    }
    let table = frame(vec![Series::new("Stock".into(), vec!["HQLA"; levels.len()]), Series::new("Level".into(), levels.clone()), Series::new("Amount (BDT)".into(), amounts)])?;
    let mut spec = base_spec("Stock", ChartKind::Bar);
    spec.series = Some("Level".into());
    spec.value = Some("Amount (BDT)".into());
    spec.stack = Stack::Stacked;
    spec.order = order_of(&["HQLA".to_string()]);
    spec.colour_map.assign_all(&["Level 1", "Level 2A", "Level 2B"]);
    spec.segment_labels = true;
    spec.subtitle = "HQLA by level, after the factors".to_string();
    let finding = format!("{}% of the HQLA is Level 1", format_with_commas(l1 / gross * 100.0, 1));
    spec.title = finding.clone();
    Ok(Some(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: HQLA composition".to_string(), companion: None }))
}

// -------------------------------------------------------------------------------------- ECL by stage

fn ecl_by_stage(df: &DataFrame) -> Result<Prepared> {
    let t = Template::EclByStage;
    let section = strs(t, df, "Section")?;
    let group = strs(t, df, "Group")?;
    let ead = nums(t, df, "EAD")?;
    let ecl = nums(t, df, "ECL")?;
    let (mut st, mut e, mut l, mut cov): (Vec<String>, Vec<f64>, Vec<f64>, Vec<f64>) = Default::default();
    for i in 0..df.height() {
        if section[i].as_deref() != Some("Stage") {
            continue;
        }
        let (Some(g), Some(a), Some(b)) = (group[i].clone(), ead[i], ecl[i]) else { continue };
        if g == "Total" {
            continue;
        }
        st.push(g);
        e.push(a);
        l.push(b);
        cov.push(if a != 0.0 { b / a * 100.0 } else { 0.0 });
    }
    if st.is_empty() {
        bail!("{}: the summary has no stage rows. Build the IFRS 9 summary first.", t.name());
    }
    let table = frame(vec![Series::new("Stage".into(), st.clone()), Series::new("EAD".into(), e), Series::new("ECL".into(), l.clone()), Series::new("Coverage %".into(), cov)])?;
    let mut spec = base_spec("Stage", ChartKind::Bar);
    spec.order = order_of(&st);
    spec.value = Some("EAD".into());
    spec.measures = vec![measure("EAD", "EAD", false), measure("ECL", "ECL", false), measure("Coverage %", "Coverage %", true)];
    spec.panels = (0..3).map(|i| PanelSpec { kind: ChartKind::Bar, measures: vec![i], title: ["EAD", "ECL", "Coverage % (ECL / EAD)"][i].to_string(), single: false }).collect();
    spec.subtitle = "IFRS 9 by stage".to_string();
    spec.footnote = with_sample("", sample_text(df, "Section", "Group"));
    let total: f64 = l.iter().sum();
    let (top_i, top) = l.iter().enumerate().fold((0usize, f64::MIN), |a, (i, v)| if *v > a.1 { (i, *v) } else { a });
    let finding = if total <= 0.0 { "No expected credit loss is held in any stage".to_string() } else { format!("{} holds {}% of ECL", st[top_i], format_with_commas(top / total * 100.0, 1)) };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: ECL by stage".to_string(), companion: None })
}

// ------------------------------------------------------------------------- tabs charted as they are

fn deposit_mix(df: &DataFrame) -> Result<Prepared> {
    let t = Template::DepositMix;
    let (month_c, prod_c, bal_c) = mix_cols(df).ok_or_else(|| anyhow!("{}: the tab needs a month column, a product column and an amount column (balance, outstanding or amount).", t.name()))?;
    let dates = month_dates(t, df, &month_c)?;
    let months = strs(t, df, &month_c)?;
    let prods = strs(t, df, &prod_c)?;
    let bal = nums(t, df, &bal_c)?;
    // by date (never by text); the tab's own month texts, in date order, are the order of the columns
    let mut by: std::collections::BTreeMap<chrono::NaiveDate, std::collections::BTreeMap<String, f64>> = std::collections::BTreeMap::new();
    let mut texts: std::collections::BTreeMap<chrono::NaiveDate, Vec<String>> = std::collections::BTreeMap::new();
    for i in 0..df.height() {
        let (Some(d), Some(p), Some(b)) = (dates[i], prods[i].clone(), bal[i]) else { continue };
        *by.entry(d).or_default().entry(p).or_default() += b;
        if let Some(m) = months[i].clone() {
            let e = texts.entry(d).or_default();
            if !e.contains(&m) {
                e.push(m);
            }
        }
    }
    if by.is_empty() {
        bail!("{}: no row has a month, a product and an amount.", t.name());
    }
    let month_order: Vec<String> = texts.into_values().flatten().collect();
    let mut spec = base_spec(&month_c, ChartKind::Bar);
    spec.series = Some(prod_c.clone());
    spec.value = Some(bal_c.clone());
    spec.stack = Stack::Stacked;
    spec.subtitle = format!("{} by {} and {}", bal_c, prod_c.to_lowercase(), month_c.to_lowercase());
    spec.order = order_of(&month_order);
    let (first, last) = (by.values().next().cloned().unwrap_or_default(), by.values().next_back().cloned().unwrap_or_default());
    let finding = if by.len() < 2 {
        let (p, _) = last.iter().fold((String::new(), f64::MIN), |a, (k, v)| if *v > a.1 { (k.clone(), *v) } else { a });
        format!("{} is the largest part", p)
    } else {
        let all: std::collections::BTreeSet<&String> = first.keys().chain(last.keys()).collect();
        let change: Vec<(String, f64)> = all.iter().map(|p| ((*p).clone(), last.get(*p).copied().unwrap_or(0.0) - first.get(*p).copied().unwrap_or(0.0))).collect();
        let total: f64 = change.iter().map(|c| c.1).sum();
        if total >= 0.0 {
            let (p, _) = change.iter().fold((String::new(), f64::MIN), |a, c| if c.1 > a.1 { (c.0.clone(), c.1) } else { a });
            format!("{} drove growth", p)
        } else {
            let (p, _) = change.iter().fold((String::new(), f64::MAX), |a, c| if c.1 < a.1 { (c.0.clone(), c.1) } else { a });
            format!("{} drove the decline", p)
        }
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: None, spec, finding, tab_title: String::new(), companion: None })
}

fn rate_profile(df: &DataFrame) -> Result<Prepared> {
    let t = Template::RateProfile;
    let band_c = rate_band_col(df).ok_or_else(|| anyhow!("{}: the tab has no rate-band column. Add one with Banking \u{25b8} Rate profile (rate bands) first.", t.name()))?;
    let bal_c = amount_col(df).ok_or_else(|| anyhow!("{}: the tab has no balance column (a number column called balance, outstanding or amount).", t.name()))?;
    let bands = strs(t, df, &band_c)?;
    let bal = nums(t, df, &bal_c)?;
    let mut by: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for i in 0..df.height() {
        if let (Some(b), Some(v)) = (bands[i].clone(), bal[i]) {
            *by.entry(b).or_default() += v;
        }
    }
    let total: f64 = by.values().sum();
    let mut spec = base_spec(&band_c, ChartKind::Bar);
    spec.value = Some(bal_c.clone());
    spec.top_n = 200;
    spec.subtitle = format!("{} by rate band", bal_c);
    let finding = match by.iter().fold(None, |a: Option<(&String, f64)>, (k, v)| if a.is_none_or(|x| *v > x.1) { Some((k, *v)) } else { a }) {
        Some((band, v)) if total > 0.0 => {
            // "09. 8%–9%" reads "8%–9%"
            let plain = match band.split_once(". ") {
                Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
                _ => band.clone(),
            };
            format!("{}% of balances sit at {}", format_with_commas(v / total * 100.0, 1), plain)
        }
        _ => "No balance sits in any rate band".to_string(),
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: None, spec, finding, tab_title: String::new(), companion: None })
}

fn behaviour(df: &DataFrame) -> Result<Prepared> {
    let t = Template::BehaviourProfile;
    let (date_c, bal_c) = history_cols(df).ok_or_else(|| anyhow!("{}: the tab needs a date column (typed as a Date) and a balance column.", t.name()))?;
    let dates = strs(t, df, &date_c)?;
    let bal = nums(t, df, &bal_c)?;
    // per date, then the last date of each month (daily history is reduced to month-ends)
    let mut per_date: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for i in 0..df.height() {
        if let (Some(d), Some(b)) = (dates[i].clone(), bal[i]) {
            *per_date.entry(d).or_default() += b;
        }
    }
    let mut per_month: std::collections::BTreeMap<String, (String, f64)> = std::collections::BTreeMap::new();
    for (d, b) in &per_date {
        let key = d.chars().take(7).collect::<String>();
        per_month.insert(key, (d.clone(), *b));
    }
    let series: Vec<(String, f64)> = per_month.values().map(|(d, b)| (month_label(d), *b)).collect();
    if series.len() < 3 {
        bail!("{}: the core level needs at least 3 month-ends of history; the tab has {}.", t.name(), series.len());
    }
    let balances: Vec<f64> = series.iter().map(|s| s.1).collect();
    let (_, core_pct) = crate::behaviour::core_volatility("the balance history", &balances, crate::behaviour::DEFAULT_Z)?;
    let latest = balances[balances.len() - 1];
    let labels: Vec<String> = series.iter().map(|s| s.0.clone()).collect();
    let table = frame(vec![Series::new("Month".into(), labels.clone()), Series::new("Balance".into(), balances.clone())])?;
    let mut spec = base_spec("Month", ChartKind::Line);
    spec.order = order_of(&labels);
    spec.value = Some("Balance".into());
    spec.ref_bands = vec![super::spec::RefBand { from: 0.0, to: latest * core_pct / 100.0, label: "Core balance".into(), panel: 0 }];
    spec.subtitle = "Balance history and core level (volatility method)".to_string();
    let finding = format!("Core balance {}% of the latest", format_with_commas(core_pct, 0));
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: balance history".to_string(), companion: None })
}

fn data_quality(df: &DataFrame, ctx: &Ctx) -> Result<Prepared> {
    let t = Template::DataQuality;
    let rule = strs(t, df, "Rule")?;
    let pass = nums(t, df, "Pass %")?;
    let (mut names, mut vals): (Vec<String>, Vec<f64>) = Default::default();
    for i in 0..df.height() {
        let (Some(r), Some(p)) = (rule[i].clone(), pass[i]) else { continue };
        if r == "Overall score" {
            continue;
        }
        names.push(r);
        vals.push(p);
    }
    if names.is_empty() {
        bail!("{}: no rule has a pass %. Run a rule that checks rows (control totals only have a pass or fail).", t.name());
    }
    let (green, amber) = match (ctx.dq_green, ctx.dq_amber) {
        (Some(g), Some(a)) if g.is_finite() && a.is_finite() && a <= g => (g, a),
        _ => bail!("{}: the Green and Amber thresholds come from the data-quality rule set the scorecard was run with, and none was given. Chart the scorecard's own result tab (or add the scorecard as the pack item); no threshold is assumed.", t.name()),
    };
    let table = frame(vec![Series::new("Rule".into(), names.clone()), Series::new("Pass %".into(), vals.clone())])?;
    let mut spec = base_spec("Rule", ChartKind::Bar);
    spec.horizontal = true;
    spec.order = order_of(&names);
    spec.value = Some("Pass %".into());
    spec.measures = vec![measure("Pass %", "Pass %", true)];
    spec.ref_lines = vec![
        RefLine { value: green, label: format!("Green {}%", format_with_commas(green, 0)), status: Status::Good, panel: 0 },
        RefLine { value: amber, label: format!("Amber {}%", format_with_commas(amber, 0)), status: Status::Warning, panel: 0 },
    ];
    spec.subtitle = "Data-quality scorecard, pass % by rule".to_string();
    // below Amber: the critical colour, an icon and "Below Amber"; between Amber and Green: the warning colour
    // and "Below Green"; the other bars keep the default colour
    for (n, v) in names.iter().zip(vals.iter()) {
        if *v < amber {
            spec.bar_status.push(BarStatus { category: n.clone(), status: Status::Critical, label: "Below Amber".to_string() });
        } else if *v < green {
            spec.bar_status.push(BarStatus { category: n.clone(), status: Status::Warning, label: "Below Green".to_string() });
        }
    }
    let below = vals.iter().filter(|v| **v < amber).count();
    let finding = match below {
        0 => "No rule is below Amber".to_string(),
        1 => "1 rule below Amber".to_string(),
        n => format!("{} rules below Amber", n),
    };
    spec.title = finding.clone();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: data quality".to_string(), companion: None })
}

// ----------------------------------------------------------------------------------- yield curves

/// A curve of the store as the template reads it: its date and its (tenor in days, rate in percent) points.
#[derive(Clone, Debug)]
pub struct CurveInput {
    pub date: chrono::NaiveDate,
    pub points: Vec<(i64, f64)>,
}

impl From<&crate::curves::YieldCurve> for CurveInput {
    fn from(c: &crate::curves::YieldCurve) -> Self {
        CurveInput { date: c.date, points: c.points.iter().map(|p| (p.tenor_days, p.ftp_rate)).collect() }
    }
}

/// The yield curves of the store, one line per curve date.
pub fn yield_curves(curves: &[CurveInput]) -> Result<Prepared> {
    let t = Template::YieldCurves;
    if curves.is_empty() {
        bail!("{}: the curve store has no curve. Import or type a curve in Banking \u{25b8} Yield curves first.", t.name());
    }
    let mut curves: Vec<&CurveInput> = curves.iter().collect();
    curves.sort_by_key(|c| c.date);
    let days: std::collections::BTreeSet<i64> = curves.iter().flat_map(|c| c.points.iter().map(|p| p.0)).collect();
    let label = |d: i64| -> String {
        if d % 365 == 0 {
            format!("{}Y", d / 365)
        } else if d % 30 == 0 && d < 365 {
            format!("{}M", d / 30)
        } else {
            format!("{}D", d)
        }
    };
    let tenors: Vec<String> = days.iter().map(|d| label(*d)).collect();
    let date_text = |c: &CurveInput| c.date.format("%-d %b %Y").to_string();
    let (mut tn, mut dt, mut rt): (Vec<String>, Vec<String>, Vec<f64>) = Default::default();
    for c in &curves {
        for (d, r) in &c.points {
            tn.push(label(*d));
            dt.push(date_text(c));
            rt.push(*r);
        }
    }
    let table = frame(vec![Series::new("Tenor".into(), tn), Series::new("Curve date".into(), dt), Series::new("Rate %".into(), rt)])?;
    let mut spec = base_spec("Tenor", ChartKind::Line);
    spec.series = Some("Curve date".into());
    spec.value = Some("Rate %".into());
    spec.agg = ChartAgg::Average;
    spec.order = order_of(&tenors);
    spec.measures = vec![MeasureSpec { column: Some("Rate %".into()), agg: ChartAgg::Average, label: "Rate %".into(), pct: true, ..Default::default() }];
    // the latest curve is blue with markers; the older ones are two greys, the older the fainter
    spec.highlight = curves.last().map(|c| date_text(c));
    spec.pad_axis_below = true;
    if let Some(latest) = curves.last() {
        spec.colour_map.assign(&date_text(latest));
    }
    let older: Vec<String> = curves.iter().take(curves.len().saturating_sub(1)).map(|c| date_text(c)).collect();
    let faint = older.len() / 2; // the oldest half takes the fainter grey, the more recent older curves the darker one
    for (i, name) in older.iter().enumerate() {
        spec.colour_map.set_grey(name, if i < faint { 1 } else { 0 });
    }
    spec.subtitle = "Yield curves by curve date".to_string();
    spec.scale = ChartScale::Plain;
    // the finding: the tenor that moved most between the oldest and the latest curve
    let (first, last) = (curves[0], curves[curves.len() - 1]);
    let finding = if curves.len() < 2 {
        format!("One curve, dated {}", date_text(last))
    } else {
        let mut best: Option<(f64, i64)> = None;
        for (d, r1) in &last.points {
            if let Some((_, r0)) = first.points.iter().find(|p| p.0 == *d) {
                let bp = (r1 - r0) * 100.0;
                if best.is_none_or(|b| bp.abs() > b.0.abs()) {
                    best = Some((bp, *d));
                }
            }
        }
        match best {
            Some((bp, d)) if bp.abs() >= 0.5 => format!("The curve {} by {} bp at {} since {}", if bp > 0.0 { "rose" } else { "fell" }, format_with_commas(bp.abs(), 0), label(d), date_text(first)),
            _ => format!("The curve is unchanged since {}", date_text(first)),
        }
    };
    spec.title = finding.clone();
    spec.template = t.name().to_string();
    Ok(Prepared { template: t, table: Some(table), spec, finding, tab_title: "Chart: yield curves".to_string(), companion: None })
}
