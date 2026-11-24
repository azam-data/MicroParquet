//! Saved tool runs (Phase 6 P1): everything a tool's dialog was given, in a file, so the same
//! result tab can be made again (`portable_data/runs/<name>.json`) and report packs (P2) can use it.
//!
//! A run never holds a tab index. Source tabs are named (the tab's title, which is its file name
//! for a tab opened from a file, or an input slot of a report pack); the names are turned back
//! into tabs through a [`RunEnv`]. Rule, curve, beta, behaviour and map files are named too and
//! read again when the run is made, exactly as the dialog does.

use crate::banking::{ClMigrationSpec, GapBehaviour, GapInputSpec, RateShockSpec};
use crate::cashflow::{CashFlowMode, CfInputSpec, Scenario};
use crate::cl1::Cl1Spec;
use crate::ecl::EclSummarySpec;
use crate::irrbb::{Compounding, IrrbbSpec};
use crate::liquidity::LiqSpec;
use crate::models::*;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The folder of the saved runs, inside `portable_data`.
pub const RUNS_FOLDER: &str = "runs";

pub fn runs_dir() -> PathBuf {
    crate::rules::data_dir().join(RUNS_FOLDER)
}

/// A source tab as a run needs it: where its rows come from and the steps that run.
#[derive(Clone, Debug)]
pub struct RunTab {
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
}

/// What a run needs from the app: the tabs by name and the folders of `portable_data`.
pub trait RunEnv {
    /// The tab (or report-pack slot) called `name`.
    fn tab(&self, name: &str) -> Result<RunTab>;
    /// The name of the tab whose source and steps these are.
    fn name_of(&self, source: &TabSource, steps: &[TransformStep]) -> Result<String>;
    fn paths(&self) -> &PortablePaths;
    /// Which file the source called `name` is (path and blake3 hash), when it is one. Tabs made by a tool have none.
    fn source_id(&self, _name: &str) -> Option<SourceId> {
        None
    }
    /// The source this identity names, now: its name here, and a warning when the file changed since it was saved.
    /// Refuses a source that is not open, or open more than once.
    fn resolve(&self, id: &SourceId) -> Result<(String, Option<String>)> {
        Ok((id.name.clone(), None))
    }
}

/// A source of a run as it was when the run was saved: its title, and the file with its content hash.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SourceId {
    pub name: String,
    pub path: String,
    pub blake3: String,
}

/// A rule, curve, profile or parameter file a run read, with its hash when it was saved.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FileHash {
    pub kind: String,
    /// As the Cover names it (`irrbb_shocks/2021-04-13.json`), under `portable_data`.
    pub file: String,
    pub blake3: String,
}

/// The hashes of the files a run read (those that can be read now).
pub fn hash_used_files(used: &[crate::rules::UsedFile]) -> Vec<FileHash> {
    let root = crate::rules::data_dir();
    used.iter()
        .filter_map(|u| crate::packs::file_hash(&root.join(&u.file)).ok().map(|h| FileHash { kind: u.kind.clone(), file: u.file.clone(), blake3: h }))
        .collect()
}

/// Warnings for the saved files that have changed (or are gone) since the run was saved.
pub fn changed_files_warning(saved: &[FileHash]) -> Option<String> {
    let root = crate::rules::data_dir();
    let changed: Vec<String> = saved
        .iter()
        .filter(|f| crate::packs::file_hash(&root.join(&f.file)).map(|h| h != f.blake3).unwrap_or(true))
        .map(|f| f.file.clone())
        .collect();
    if changed.is_empty() {
        None
    } else {
        Some(format!("These files changed since the run was saved, so the numbers may differ from last time: {}.", changed.join(", ")))
    }
}

fn name_of_base(env: &dyn RunEnv, source: &TabSource, steps: &[TransformStep]) -> Result<String> {
    env.name_of(source, steps)
}

// ------------------------------------------------------------------------------ the configs

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RepricingGapRun {
    pub source: String,
    pub spec: RateShockSpec,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct GapInputRun {
    pub source: String,
    pub spec: GapInputSpec,
    /// The product code column, read against the behaviour profile ("" when none is applied).
    pub product_col: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct GapLadderRun {
    pub inputs: Vec<GapInputRun>,
    /// The bucket preset's name, looked up in the presets of `buckets.json` and the built-in ones.
    pub preset: String,
    /// The preset as it was when the run was saved; used only when no preset of that name exists any more.
    pub preset_def: Option<CustomBucketDef>,
    pub cutoff: String,
    pub null_as_sight: bool,
    /// The behaviour profile's file name ("" = none, the Phase 4 ladder).
    pub behaviour_file: String,
    /// "Stress: early withdrawal (TDRR)" (needs the behaviour profile).
    pub stress_tdrr: bool,
}

impl Default for GapLadderRun {
    fn default() -> Self {
        Self { inputs: Vec::new(), preset: String::new(), preset_def: None, cutoff: String::new(), null_as_sight: true, behaviour_file: String::new(), stress_tdrr: false }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct CfInputRun {
    pub source: String,
    pub spec: CfInputSpec,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct IrrbbLadderRun {
    pub inputs: Vec<CfInputRun>,
    pub cutoff: String,
    pub mode: CashFlowMode,
    pub scenario: Scenario,
    pub behaviour_file: String,
    pub blank_maturity_on: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct IrrbbSixRun {
    pub inputs: Vec<CfInputRun>,
    pub cutoff: String,
    pub mode: CashFlowMode,
    pub blank_maturity_on: bool,
    /// Tier 1 capital in BDT (Tier 1 is an input of the run, so it is saved with it).
    pub tier1: f64,
    pub compounding: Compounding,
    pub post_shock_floor_pct: Option<f64>,
    pub nii_floor_pct: Option<f64>,
    pub behaviour_file: String,
    pub beta_file: String,
    /// Which of the run's two tables this tab shows.
    pub view: IrrbbView,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct ClMigrationRun {
    pub prev: String,
    pub curr: String,
    pub spec: ClMigrationSpec,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct MomVarianceRun {
    pub prev: String,
    pub curr: String,
    pub spec: crate::mom::MomSpec,
    /// Which of the run's two tables this tab shows.
    pub view: MomView,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct MudarabaRun {
    pub source: String,
    pub spec: crate::islamic::MudarabaSpec,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct DqScorecardRun {
    pub source: String,
    /// The rules as they were run (a rule set typed in the dialog may not be saved as a file).
    pub rules: crate::dq::DqRuleSet,
    pub view: DqView,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Cl1SummaryRun {
    pub source: String,
    pub spec: Cl1Spec,
    /// The report template used (`report_templates/cl1.json`), for the record.
    pub template_file: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct LiquidityRun {
    pub source: String,
    pub spec: LiqSpec,
    /// The map file in `portable_data/liquidity_maps`.
    pub map_file: String,
    pub view: LiquidityView,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct EclSummaryRun {
    pub source: String,
    /// The IFRS 9 ECL step (its dialog inputs, Tier 1 of the ECL rule files included) that is added to the source's steps.
    pub ecl_step: Option<TransformStep>,
    pub spec: EclSummarySpec,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct PeriodReconcileRun {
    pub prev: String,
    pub curr: String,
    pub key_col: String,
    pub bal_col: String,
    pub rate_col: Option<String>,
    pub cl_status_col: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct JoinRun {
    pub left: String,
    pub right: String,
    pub kind: SqlJoinKind,
    pub keys: Vec<String>,
    pub right_cols_to_keep: Vec<String>,
    pub keep_first_duplicate_right: bool,
}

impl Default for JoinRun {
    fn default() -> Self {
        Self { left: String::new(), right: String::new(), kind: SqlJoinKind::Left, keys: Vec::new(), right_cols_to_keep: Vec::new(), keep_first_duplicate_right: false }
    }
}

/// One file of a stack, with the schema it is read with.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StackFileRun {
    pub path: PathBuf,
    pub is_parquet: bool,
    pub config: IngestionConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StackRun {
    pub files: Vec<StackFileRun>,
    pub include_source_col: bool,
}

/// A pivot made into a new tab: the tab it was made from and the pivot step.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct PivotTabRun {
    pub source: String,
    pub pivot: Option<TransformStep>,
}

/// The inputs of one tool that makes a result tab.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolRun {
    RepricingGap(RepricingGapRun),
    GapLadder(GapLadderRun),
    IrrbbLadder(IrrbbLadderRun),
    IrrbbSix(IrrbbSixRun),
    ClMigration(ClMigrationRun),
    MomVariance(MomVarianceRun),
    DqScorecard(DqScorecardRun),
    Mudaraba(MudarabaRun),
    Cl1Summary(Cl1SummaryRun),
    Liquidity(LiquidityRun),
    EclSummary(EclSummaryRun),
    PeriodReconcile(PeriodReconcileRun),
    Join(JoinRun),
    Stack(StackRun),
    PivotTab(PivotTabRun),
}

impl Default for ToolRun {
    fn default() -> Self {
        ToolRun::Stack(StackRun::default())
    }
}

/// A run in its file: the name it was saved under and the version of the app that saved it.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SavedRun {
    pub name: String,
    pub app_version: String,
    pub run: ToolRun,
    /// The file (path and hash) behind each source tab, so the run finds the same file again.
    pub sources: Vec<SourceId>,
    /// The rule, curve, profile and parameter files the run read, with their hashes.
    pub files: Vec<FileHash>,
}

const NOT_A_TOOL: &str = "This tab was not made by a tool whose run can be saved (repricing gap, gap ladder, IRRBB, CL migration, CL-1, LCR/NSFR, IFRS 9 summary, period reconcile, join, stack or pivot tab).";

fn is_pivot_step(s: &TransformStep) -> bool {
    matches!(s.inner(), TransformStep::GroupByPivot { .. } | TransformStep::PivotTable { .. })
}

fn no_extra_steps(tab_steps: &[TransformStep]) -> Result<()> {
    if tab_steps.is_empty() {
        Ok(())
    } else {
        bail!("This tab has steps of its own, which a saved run does not keep. Save the steps as a macro, or save the run before adding steps.")
    }
}

fn blank_to_none(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

impl ToolRun {
    /// The tool's name as the menus say it.
    pub fn tool_name(&self) -> &'static str {
        match self {
            ToolRun::RepricingGap(_) => "Repricing gap",
            ToolRun::GapLadder(_) => "Gap ladder",
            ToolRun::IrrbbLadder(_) => "IRRBB cash-flow ladder",
            ToolRun::IrrbbSix(_) => "IRRBB (six scenarios)",
            ToolRun::ClMigration(_) => "CL migration",
            ToolRun::MomVariance(_) => "MoM variance",
            ToolRun::DqScorecard(_) => "Data-quality scorecard",
            ToolRun::Mudaraba(_) => "Mudaraba profit distribution",
            ToolRun::Cl1Summary(_) => "CL-1 summary",
            ToolRun::Liquidity(_) => "LCR/NSFR",
            ToolRun::EclSummary(_) => "IFRS 9 ECL summary",
            ToolRun::PeriodReconcile(_) => "Period reconcile",
            ToolRun::Join(_) => "Join",
            ToolRun::Stack(_) => "Stack",
            ToolRun::PivotTab(_) => "Pivot tab",
        }
    }

    /// The names of the source tabs the run reads, in the order of its inputs.
    /// The file identity of each source of the run (those that are files).
    pub fn source_ids(&self, env: &dyn RunEnv) -> Vec<SourceId> {
        let mut out: Vec<SourceId> = Vec::new();
        for n in self.source_names() {
            if let Some(id) = env.source_id(&n) {
                if !out.iter().any(|o| o.name == id.name) {
                    out.push(id);
                }
            }
        }
        out
    }

    /// Points each source at the open tab of the same file (by path), whatever the tab is called now.
    /// Returns the warnings (a file that changed since the run was saved).
    pub fn resolve_sources(&mut self, ids: &[SourceId], env: &dyn RunEnv) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        let mut renames: Vec<(String, String)> = Vec::new();
        for id in ids.iter().filter(|i| !i.path.is_empty()) {
            let (now, warn) = env.resolve(id)?;
            warnings.extend(warn);
            renames.push((id.name.clone(), now));
        }
        self.each_source_name(&mut |n| {
            if let Some((_, now)) = renames.iter().find(|(old, _)| old == n) {
                *n = now.clone();
            }
        });
        Ok(warnings)
    }

    pub fn source_names(&self) -> Vec<String> {
        let mut c = self.clone();
        let mut v = Vec::new();
        c.each_source_name(&mut |n| v.push(n.clone()));
        v
    }

    /// Calls `f` on each source name (report packs point them at their input slots).
    pub fn each_source_name(&mut self, f: &mut dyn FnMut(&mut String)) {
        match self {
            ToolRun::RepricingGap(r) => f(&mut r.source),
            ToolRun::GapLadder(r) => r.inputs.iter_mut().for_each(|i| f(&mut i.source)),
            ToolRun::IrrbbLadder(r) => r.inputs.iter_mut().for_each(|i| f(&mut i.source)),
            ToolRun::IrrbbSix(r) => r.inputs.iter_mut().for_each(|i| f(&mut i.source)),
            ToolRun::ClMigration(r) => {
                f(&mut r.prev);
                f(&mut r.curr);
            }
            ToolRun::MomVariance(r) => {
                f(&mut r.prev);
                f(&mut r.curr);
            }
            ToolRun::DqScorecard(r) => f(&mut r.source),
            ToolRun::Mudaraba(r) => f(&mut r.source),
            ToolRun::Cl1Summary(r) => f(&mut r.source),
            ToolRun::Liquidity(r) => f(&mut r.source),
            ToolRun::EclSummary(r) => f(&mut r.source),
            ToolRun::PeriodReconcile(r) => {
                f(&mut r.prev);
                f(&mut r.curr);
            }
            ToolRun::Join(r) => {
                f(&mut r.left);
                f(&mut r.right);
            }
            ToolRun::Stack(_) => {}
            ToolRun::PivotTab(r) => f(&mut r.source),
        }
    }

    /// Sets the cutoff date of a tool that has one (a report pack runs everything at its cutoff).
    /// Returns false for a tool without a cutoff date.
    pub fn set_cutoff(&mut self, cutoff: &str) -> bool {
        let c = cutoff.trim().to_string();
        match self {
            ToolRun::RepricingGap(r) => r.spec.cutoff_date = c,
            ToolRun::GapLadder(r) => r.cutoff = c,
            ToolRun::IrrbbLadder(r) => r.cutoff = c,
            ToolRun::IrrbbSix(r) => r.cutoff = c,
            ToolRun::EclSummary(r) => match &mut r.ecl_step {
                Some(TransformStep::IfrsEcl { cutoff_date, .. }) => *cutoff_date = c,
                _ => return false,
            },
            _ => return false,
        }
        true
    }

    /// The run behind a result tab: its source and the steps the tab itself has.
    pub fn from_source(source: &TabSource, tab_steps: &[TransformStep], env: &dyn RunEnv) -> Result<ToolRun> {
        // a pivot made into a new tab: the tab it was made from, then the pivot step
        if tab_steps.last().is_some_and(is_pivot_step) {
            let base = &tab_steps[..tab_steps.len() - 1];
            return Ok(ToolRun::PivotTab(PivotTabRun { source: name_of_base(env, source, base).context("The tab the pivot was made from")?, pivot: tab_steps.last().cloned() }));
        }
        let name = |s: &TabSource, st: &[TransformStep]| name_of_base(env, s, st);
        match source {
            TabSource::RepricingGap { source, steps, spec } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::RepricingGap(RepricingGapRun { source: name(source, steps)?, spec: spec.clone() }))
            }
            TabSource::GapLadder { inputs, preset, cutoff_date, null_as_sight, behaviour, .. } => {
                no_extra_steps(tab_steps)?;
                let mut runs = Vec::new();
                for (i, inp) in inputs.iter().enumerate() {
                    let product_col = behaviour.as_ref().and_then(|b| b.product_cols.get(i).cloned()).unwrap_or_default();
                    runs.push(GapInputRun { source: name(&inp.source, &inp.steps)?, spec: inp.spec.clone(), product_col });
                }
                Ok(ToolRun::GapLadder(GapLadderRun { inputs: runs, preset: preset.name.clone(), preset_def: Some(preset.clone()), cutoff: cutoff_date.clone(), null_as_sight: *null_as_sight, behaviour_file: behaviour.as_ref().map(|b| b.file_name.clone()).unwrap_or_default(), stress_tdrr: behaviour.as_ref().is_some_and(|b| b.stress_tdrr) }))
            }
            TabSource::IrrbbLadder { inputs, cutoff_date, mode, scenario, behaviour, blank_maturity_on } => {
                no_extra_steps(tab_steps)?;
                let mut runs = Vec::new();
                for inp in inputs {
                    runs.push(CfInputRun { source: name(&inp.source, &inp.steps)?, spec: inp.spec.clone() });
                }
                Ok(ToolRun::IrrbbLadder(IrrbbLadderRun { inputs: runs, cutoff: cutoff_date.clone(), mode: *mode, scenario: *scenario, behaviour_file: behaviour.as_ref().map(|b| b.0.clone()).unwrap_or_default(), blank_maturity_on: *blank_maturity_on }))
            }
            TabSource::Irrbb { inputs, spec, view } => {
                no_extra_steps(tab_steps)?;
                let mut runs = Vec::new();
                for inp in inputs {
                    runs.push(CfInputRun { source: name(&inp.source, &inp.steps)?, spec: inp.spec.clone() });
                }
                Ok(ToolRun::IrrbbSix(IrrbbSixRun {
                    inputs: runs,
                    cutoff: spec.cutoff.format("%Y-%m-%d").to_string(),
                    mode: spec.mode,
                    blank_maturity_on: spec.blank_maturity_on,
                    tier1: spec.tier1,
                    compounding: spec.compounding,
                    post_shock_floor_pct: spec.post_shock_floor_pct,
                    nii_floor_pct: spec.nii_floor_pct,
                    behaviour_file: spec.behaviour.as_ref().map(|b| b.0.clone()).unwrap_or_default(),
                    beta_file: spec.beta_file.clone(),
                    view: *view,
                }))
            }
            TabSource::ClMigration { prev_source, prev_steps, curr_source, curr_steps, spec } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::ClMigration(ClMigrationRun { prev: name(prev_source, prev_steps)?, curr: name(curr_source, curr_steps)?, spec: spec.clone() }))
            }
            TabSource::MomVariance { prev_source, prev_steps, curr_source, curr_steps, spec, view } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::MomVariance(MomVarianceRun { prev: name(prev_source, prev_steps)?, curr: name(curr_source, curr_steps)?, spec: spec.clone(), view: *view }))
            }
            TabSource::MudarabaPool { source, steps, spec } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::Mudaraba(MudarabaRun { source: name(source, steps)?, spec: spec.clone() }))
            }
            TabSource::DqScorecard { source, steps, rules, view } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::DqScorecard(DqScorecardRun { source: name(source, steps)?, rules: (**rules).clone(), view: *view }))
            }
            TabSource::Cl1Summary { source, steps, spec, .. } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::Cl1Summary(Cl1SummaryRun { source: name(source, steps)?, spec: spec.clone(), template_file: format!("report_templates/{}", crate::cl1::TEMPLATE_FILE) }))
            }
            TabSource::Liquidity { source, steps, spec, map_file, view, .. } => {
                no_extra_steps(tab_steps)?;
                // the file name only (a map is read from portable_data/liquidity_maps)
                let file = map_file.rsplit(['/', '\\']).next().unwrap_or(map_file).to_string();
                Ok(ToolRun::Liquidity(LiquidityRun { source: name(source, steps)?, spec: spec.clone(), map_file: file, view: *view }))
            }
            TabSource::EclSummary { source, steps, spec } => {
                no_extra_steps(tab_steps)?;
                // the ECL step was added to the tab the summary reads: the run keeps it apart
                let (base, ecl_step) = match steps.last() {
                    Some(s @ TransformStep::IfrsEcl { .. }) => (&steps[..steps.len() - 1], Some(s.clone())),
                    _ => (&steps[..], None),
                };
                Ok(ToolRun::EclSummary(EclSummaryRun { source: name(source, base)?, ecl_step, spec: spec.clone() }))
            }
            TabSource::PeriodReconcile { prev_source, prev_steps, curr_source, curr_steps, key_col, bal_col, rate_col, cl_status_col } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::PeriodReconcile(PeriodReconcileRun { prev: name(prev_source, prev_steps)?, curr: name(curr_source, curr_steps)?, key_col: key_col.clone(), bal_col: bal_col.clone(), rate_col: rate_col.clone(), cl_status_col: cl_status_col.clone() }))
            }
            TabSource::JoinedTabs { left_source, left_steps, right_source, right_steps, join_kind, join_keys, right_cols_to_keep, keep_first_duplicate_right } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::Join(JoinRun { left: name(left_source, left_steps)?, right: name(right_source, right_steps)?, kind: *join_kind, keys: join_keys.clone(), right_cols_to_keep: right_cols_to_keep.clone(), keep_first_duplicate_right: *keep_first_duplicate_right }))
            }
            TabSource::VerticalStack { files, include_source_col } => {
                no_extra_steps(tab_steps)?;
                Ok(ToolRun::Stack(StackRun { files: files.iter().map(|(p, q, c)| StackFileRun { path: p.clone(), is_parquet: *q, config: c.clone() }).collect(), include_source_col: *include_source_col }))
            }
            TabSource::SingleFile { .. } | TabSource::Unavailable { .. } => bail!("{}", NOT_A_TOOL),
        }
    }

    /// The tab or tabs the run makes, each with its title, from the open tabs and the files of `portable_data`.
    /// Every problem (a tab that is not open, a file that is missing, a column that is not there) is an error
    /// that says what to do; nothing is made from part of a run.
    pub fn to_sources(&self, env: &dyn RunEnv) -> Result<Vec<(String, TabSource)>> {
        let paths = env.paths();
        let tab = |name: &str| -> Result<RunTab> {
            if name.trim().is_empty() {
                bail!("The saved run has no source tab for one of its inputs. Choose it in the dialog.");
            }
            env.tab(name)
        };
        match self {
            ToolRun::RepricingGap(r) => {
                let t = tab(&r.source)?;
                Ok(vec![(format!("Repricing gap ({})", r.source), TabSource::RepricingGap { source: Box::new(t.source), steps: t.steps, spec: r.spec.clone() })])
            }
            ToolRun::GapLadder(r) => {
                if chrono::NaiveDate::parse_from_str(r.cutoff.trim(), "%Y-%m-%d").is_err() {
                    bail!("The saved run's cutoff date '{}' is not YYYY-MM-DD (for example 2026-10-31).", r.cutoff);
                }
                if r.inputs.is_empty() {
                    bail!("The saved gap ladder has no input tabs.");
                }
                let presets = CustomBucketDef::load_all_presets(&paths.buckets_file);
                let preset = presets
                    .into_iter()
                    .find(|p| p.name == r.preset)
                    .or_else(|| r.preset_def.clone().filter(|p| p.name == r.preset))
                    .ok_or_else(|| anyhow!("The bucket preset '{}' of the saved run is not in buckets.json. Choose a preset in the dialog.", r.preset))?;
                let mut inputs = Vec::new();
                for i in &r.inputs {
                    let t = tab(&i.source)?;
                    inputs.push(GapTabInput { source: t.source, steps: t.steps, spec: i.spec.clone() });
                }
                let limits = crate::banking::load_gap_limits(&paths.gap_limits_dir(), &preset)?;
                let behaviour = if r.behaviour_file.trim().is_empty() {
                    None
                } else {
                    let mut product_cols = Vec::new();
                    for (n, i) in r.inputs.iter().enumerate() {
                        if i.product_col.trim().is_empty() {
                            bail!("Line {}: the saved run has no product column (the behaviour profile is read against it).", n + 1);
                        }
                        product_cols.push(i.product_col.clone());
                    }
                    let file = crate::behaviour::load_behaviour(&crate::rules::behaviour_dir(), &r.behaviour_file)?.data;
                    Some(GapBehaviour { file_name: r.behaviour_file.clone(), file, product_cols, stress_tdrr: r.stress_tdrr })
                };
                if r.stress_tdrr && behaviour.is_none() {
                    bail!("The saved gap ladder has the TDRR stress on but no behaviour profile. Choose a profile in the dialog.");
                }
                let title = if r.stress_tdrr { format!("Gap ladder ({}) (stressed)", preset.name) } else { format!("Gap ladder ({})", preset.name) };
                Ok(vec![(title, TabSource::GapLadder { inputs, preset, cutoff_date: r.cutoff.trim().to_string(), null_as_sight: r.null_as_sight, limits, behaviour })])
            }
            ToolRun::IrrbbLadder(r) => {
                chrono::NaiveDate::parse_from_str(r.cutoff.trim(), "%Y-%m-%d").map_err(|_| anyhow!("The saved run's cutoff date '{}' is not YYYY-MM-DD (for example 2026-10-31).", r.cutoff))?;
                let inputs = cf_inputs(&r.inputs, &tab)?;
                let behaviour = load_behaviour_opt(&r.behaviour_file)?;
                Ok(vec![("IRRBB cash-flow ladder".to_string(), TabSource::IrrbbLadder { inputs, cutoff_date: r.cutoff.trim().to_string(), mode: r.mode, scenario: r.scenario, behaviour, blank_maturity_on: r.blank_maturity_on })])
            }
            ToolRun::IrrbbSix(r) => {
                let cutoff = chrono::NaiveDate::parse_from_str(r.cutoff.trim(), "%Y-%m-%d").map_err(|_| anyhow!("The saved run's cutoff date '{}' is not YYYY-MM-DD (for example 2026-10-31).", r.cutoff))?;
                if !(r.tier1.is_finite() && r.tier1 > 0.0) {
                    bail!("IRRBB: the saved run has no Tier 1 capital. Type it in the dialog (BDT, above 0).");
                }
                let inputs = cf_inputs(&r.inputs, &tab)?;
                let shocks = crate::rules::load_shocks(cutoff)?;
                let behaviour = load_behaviour_opt(&r.behaviour_file)?;
                let betas = if r.beta_file.trim().is_empty() { Vec::new() } else { crate::betas::load_beta_rows(&crate::betas::betas_dir(), &r.beta_file)? };
                let store = crate::curves::CurveStore::load(&crate::curves::curves_dir());
                let curves = store.strict()?.to_vec();
                let spec = IrrbbSpec { cutoff, mode: r.mode, blank_maturity_on: r.blank_maturity_on, tier1: r.tier1, compounding: r.compounding, post_shock_floor_pct: r.post_shock_floor_pct, nii_floor_pct: r.nii_floor_pct, shocks, curves, behaviour, betas, beta_file: r.beta_file.clone() };
                let title = match r.view {
                    IrrbbView::Results => "IRRBB results",
                    IrrbbView::CashFlows => "IRRBB cash flows",
                };
                Ok(vec![(title.to_string(), TabSource::Irrbb { inputs, spec: Box::new(spec), view: r.view })])
            }
            ToolRun::ClMigration(r) => {
                if r.prev == r.curr {
                    bail!("The saved CL migration names the same tab for both months ('{}'). Choose two different tabs.", r.prev);
                }
                let (p, c) = (tab(&r.prev)?, tab(&r.curr)?);
                Ok(vec![(
                    format!("CL migration ({} to {})", r.prev, r.curr),
                    TabSource::ClMigration { prev_source: Box::new(p.source), prev_steps: p.steps, curr_source: Box::new(c.source), curr_steps: c.steps, spec: r.spec.clone() },
                )])
            }
            ToolRun::MomVariance(r) => {
                if r.prev == r.curr {
                    bail!("The saved MoM variance names the same tab for both months ('{}'). Choose two different tabs.", r.prev);
                }
                let (p, c) = (tab(&r.prev)?, tab(&r.curr)?);
                r.spec.check()?;
                let title = match r.view {
                    MomView::Variance => "MoM variance",
                    MomView::Commentary => "MoM commentary",
                };
                Ok(vec![(title.to_string(), TabSource::MomVariance { prev_source: Box::new(p.source), prev_steps: p.steps, curr_source: Box::new(c.source), curr_steps: c.steps, spec: r.spec.clone(), view: r.view })])
            }
            ToolRun::Mudaraba(r) => {
                let t = tab(&r.source)?;
                r.spec.dates()?;
                Ok(vec![("Mudaraba profit distribution".to_string(), TabSource::MudarabaPool { source: Box::new(t.source), steps: t.steps, spec: r.spec.clone() })])
            }
            ToolRun::DqScorecard(r) => {
                let t = tab(&r.source)?;
                r.rules.check()?;
                let title = match r.view {
                    DqView::Scorecard => "Data-quality scorecard".to_string(),
                    DqView::Failing(i) => format!("Failing rows of rule {}", i + 1),
                };
                Ok(vec![(title, TabSource::DqScorecard { source: Box::new(t.source), steps: t.steps, rules: Box::new(r.rules.clone()), view: r.view })])
            }
            ToolRun::Cl1Summary(r) => {
                let t = tab(&r.source)?;
                let template = crate::cl1::load_template()?;
                Ok(vec![("CL-1 summary".to_string(), TabSource::Cl1Summary { source: Box::new(t.source), steps: t.steps, spec: r.spec.clone(), template: Box::new(template) })])
            }
            ToolRun::Liquidity(r) => {
                let t = tab(&r.source)?;
                let map = crate::liquidity::load_map(&r.map_file)?;
                let title = match r.view {
                    LiquidityView::Lines => "LCR/NSFR lines",
                    LiquidityView::Summary => "LCR/NSFR summary",
                };
                Ok(vec![(title.to_string(), TabSource::Liquidity { source: Box::new(t.source), steps: t.steps, spec: r.spec.clone(), map: Box::new(map), map_file: r.map_file.clone(), view: r.view })])
            }
            ToolRun::EclSummary(r) => {
                let t = tab(&r.source)?;
                let mut steps = t.steps;
                match &r.ecl_step {
                    Some(s) => steps.push(s.clone()),
                    None => bail!("The saved IFRS 9 run has no ECL step. Run the IFRS 9 ECL tool again and save the run."),
                }
                Ok(vec![("IFRS 9 summary".to_string(), TabSource::EclSummary { source: Box::new(t.source), steps, spec: r.spec.clone() })])
            }
            ToolRun::PeriodReconcile(r) => {
                let (p, c) = (tab(&r.prev)?, tab(&r.curr)?);
                Ok(vec![(
                    format!("Reconcile({} vs {})", r.prev, r.curr),
                    TabSource::PeriodReconcile { prev_source: Box::new(p.source), prev_steps: p.steps, curr_source: Box::new(c.source), curr_steps: c.steps, key_col: r.key_col.trim().to_string(), bal_col: r.bal_col.trim().to_string(), rate_col: blank_to_none(&r.rate_col), cl_status_col: blank_to_none(&r.cl_status_col) },
                )])
            }
            ToolRun::Join(r) => {
                let (l, rt) = (tab(&r.left)?, tab(&r.right)?);
                Ok(vec![(
                    format!("Join({}+{})", r.left, r.right),
                    TabSource::JoinedTabs { left_source: Box::new(l.source), left_steps: l.steps, right_source: Box::new(rt.source), right_steps: rt.steps, join_kind: r.kind, join_keys: r.keys.clone(), right_cols_to_keep: r.right_cols_to_keep.clone(), keep_first_duplicate_right: r.keep_first_duplicate_right },
                )])
            }
            ToolRun::Stack(r) => {
                if r.files.is_empty() {
                    bail!("The saved stack has no files.");
                }
                let missing: Vec<String> = r.files.iter().filter(|f| !f.path.exists()).map(|f| f.path.display().to_string()).collect();
                if !missing.is_empty() {
                    bail!("The saved stack's file(s) are not there: {}. Pick the files again in the Stack dialog.", missing.join(", "));
                }
                Ok(vec![(format!("Stacked ({} files)", r.files.len()), TabSource::VerticalStack { files: r.files.iter().map(|f| (f.path.clone(), f.is_parquet, f.config.clone())).collect(), include_source_col: r.include_source_col })])
            }
            ToolRun::PivotTab(r) => {
                let t = tab(&r.source)?;
                let Some(step) = r.pivot.clone().filter(is_pivot_step) else {
                    bail!("The saved pivot has no pivot step.");
                };
                let title = match step.as_pivot_spec().and_then(|s| s.rows.first().or(s.columns.first()).cloned()) {
                    Some(f) => format!("Pivot: {}", f),
                    None => "Pivot".to_string(),
                };
                // a pivot tab keeps the source tab's source; its steps come from `tab_steps`
                Ok(vec![(title, t.source)])
            }
        }
    }
}

impl ToolRun {
    /// The steps the result tab holds itself (only a pivot tab has any: the source tab's steps and the pivot).
    pub fn tab_steps(&self, env: &dyn RunEnv) -> Result<Vec<TransformStep>> {
        match self {
            ToolRun::PivotTab(r) => {
                let t = env.tab(&r.source)?;
                let mut steps = t.steps;
                match r.pivot.clone() {
                    Some(s) if is_pivot_step(&s) => steps.push(s),
                    _ => bail!("The saved pivot has no pivot step."),
                }
                Ok(steps)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// The first tab the run makes: its title, source and own steps.
    pub fn build(&self, env: &dyn RunEnv) -> Result<Vec<(String, TabSource, Vec<TransformStep>)>> {
        let steps = self.tab_steps(env)?;
        Ok(self.to_sources(env)?.into_iter().map(|(t, s)| (t, s, steps.clone())).collect())
    }
}

fn cf_inputs(runs: &[CfInputRun], tab: &dyn Fn(&str) -> Result<RunTab>) -> Result<Vec<CfTabInput>> {
    if runs.is_empty() {
        bail!("The saved run has no input tabs.");
    }
    let mut v = Vec::new();
    for i in runs {
        let t = tab(&i.source)?;
        v.push(CfTabInput { source: t.source, steps: t.steps, spec: i.spec.clone() });
    }
    Ok(v)
}

fn load_behaviour_opt(file: &str) -> Result<Option<(String, crate::behaviour::BehaviourFile)>> {
    if file.trim().is_empty() {
        return Ok(None);
    }
    let f = crate::behaviour::load_behaviour(&crate::rules::behaviour_dir(), file)?;
    Ok(Some((file.to_string(), f.data)))
}

// ------------------------------------------------------------------------------------ the files

/// A name that can be a file name in `portable_data/runs`.
pub fn run_file_stem(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        bail!("Type a name for the run.");
    }
    if n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || n.contains("..") {
        bail!("The run name '{}' cannot be a file name. Leave out / \\ : * ? \" < > | and \"..\".", n);
    }
    Ok(n.trim_end_matches(".json").to_string())
}

pub fn run_path(dir: &Path, name: &str) -> Result<PathBuf> {
    Ok(dir.join(format!("{}.json", run_file_stem(name)?)))
}

/// Writes `portable_data/runs/<name>.json`. An existing run of that name is replaced.
pub fn save_run(dir: &Path, name: &str, run: &ToolRun) -> Result<PathBuf> {
    save_run_with(dir, name, run, Vec::new(), Vec::new())
}

/// Saves a run with the files behind its source tabs and the files it read.
pub fn save_run_with(dir: &Path, name: &str, run: &ToolRun, sources: Vec<SourceId>, files: Vec<FileHash>) -> Result<PathBuf> {
    let path = run_path(dir, name)?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    let saved = SavedRun { name: run_file_stem(name)?, app_version: env!("CARGO_PKG_VERSION").to_string(), run: run.clone(), sources, files };
    let text = serde_json::to_string_pretty(&saved)?;
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("Could not write the run {}", path.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("Could not write the run {}", path.display()))?;
    Ok(path)
}

/// Reads one saved run.
pub fn load_run(dir: &Path, name: &str) -> Result<SavedRun> {
    let path = run_path(dir, name)?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the saved run {}", path.display()))?;
    serde_json::from_str::<SavedRun>(&text).map_err(|e| anyhow!("The saved run {} is not valid: {}.", path.display(), e))
}

/// The names of the saved runs, sorted.
pub fn list_runs(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.strip_suffix(".json").map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

/// "Gap ladder: 2 tabs, cutoff 2026-10-31", for lists.
pub fn describe(run: &ToolRun) -> String {
    let n = run.source_names().len();
    match run {
        ToolRun::GapLadder(r) => format!("{}: {} tab(s), cutoff {}", run.tool_name(), n, r.cutoff),
        ToolRun::IrrbbLadder(r) => format!("{}: {} tab(s), cutoff {}", run.tool_name(), n, r.cutoff),
        ToolRun::IrrbbSix(r) => format!("{}: {} tab(s), cutoff {}", run.tool_name(), n, r.cutoff),
        _ => format!("{}: {} source tab(s)", run.tool_name(), n),
    }
}
