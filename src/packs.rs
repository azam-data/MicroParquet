//! Report packs (Phase 6 P2): a monthly report as a file. `portable_data/packs/<name>.json` names
//! the input slots (a schema and steps for each), the items that make the sheets (a slot's table, a
//! pivot on a slot, or a saved tool run whose sources are slots) and the output. Running a pack
//! takes one file per slot and the cutoff date and writes one workbook: a Cover sheet (pack, run
//! time, app version, cutoff, each input with its rows and blake3 hash, every rule, curve, profile
//! and parameter file used with its sample flag) and one sheet per item. An item that fails stops
//! the run with its name and the reason, and nothing is written.

use crate::engine::*;
use crate::exporters::{write_xlsx_tabs_noted, PdfTab, XlsxScale, XlsxTab, EXCEL_MAX_DATA_ROWS};
use crate::jobs::JobCtx;
use crate::models::*;
use crate::tool_runs::{RunEnv, RunTab, ToolRun};
use anyhow::{anyhow, bail, Context, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub const PACKS_FOLDER: &str = "packs";
/// The sheet every pack workbook starts with.
pub const COVER_SHEET: &str = "Cover";
/// The columns of the Cover sheet (all text).
pub const COVER_COLUMNS: [&str; 6] = ["Section", "Name", "Detail", "Rows", "Blake3 hash", "Sample"];

pub fn packs_dir() -> PathBuf {
    crate::rules::data_dir().join(PACKS_FOLDER)
}

/// One file the pack is given: how it is read.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct PackInput {
    pub slot: String,
    /// A saved schema in `portable_data/schemas` (its name, without `.schema.json`).
    pub schema_template: Option<String>,
    /// The schema of the tab the slot was made from (used when there is no `schema_template`).
    pub schema: Option<IngestionConfig>,
    /// The steps applied when the slot's file is loaded.
    pub steps: Vec<TransformStep>,
    /// The file's own column names when the slot was made: a file that lacks one is refused.
    pub required_columns: Vec<String>,
    /// The tab the slot was made from, so an item made from a result of that tab points at the slot.
    pub captured_from: String,
    /// The file of that tab and its blake3 hash (empty for a tab made by a tool): an item finds its slot by
    /// this file, not by a title that two tabs may share.
    pub captured_path: String,
    pub captured_hash: String,
}

/// What an item puts on its sheet.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackItemKind {
    /// The table of a slot (its file after the slot's steps).
    Steps { slot: String },
    /// A pivot on a slot.
    Pivot { slot: String, pivot: Option<TransformStep> },
    /// A saved tool run (P1) whose source names are slots.
    Run { run: ToolRun },
}

impl Default for PackItemKind {
    fn default() -> Self {
        PackItemKind::Steps { slot: String::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct PackItem {
    pub title: String,
    pub sheet: String,
    pub source: PackItemKind,
    /// The column formats the result tab had (percent columns and so on).
    pub col_formats: HashMap<String, ColumnFormatSpec>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PackOutput {
    /// Excel is always written; this adds a PDF next to it.
    pub pdf: bool,
    /// The scale money is written in.
    pub scale: NumberDisplayFormat,
    pub decimals: usize,
}

impl Default for PackOutput {
    fn default() -> Self {
        Self { pdf: false, scale: NumberDisplayFormat::BdtCrore, decimals: 2 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ReportPack {
    pub name: String,
    pub inputs: Vec<PackInput>,
    pub items: Vec<PackItem>,
    pub output: PackOutput,
}

// ---------------------------------------------------------------------------------- the files

fn pack_stem(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        bail!("Type a name for the pack.");
    }
    if n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || n.contains("..") {
        bail!("The pack name '{}' cannot be a file name. Leave out / \\ : * ? \" < > | and \"..\".", n);
    }
    Ok(n.trim_end_matches(".json").to_string())
}

pub fn pack_path(dir: &Path, name: &str) -> Result<PathBuf> {
    Ok(dir.join(format!("{}.json", pack_stem(name)?)))
}

pub fn save_pack(dir: &Path, pack: &ReportPack) -> Result<PathBuf> {
    validate_pack(pack)?;
    let path = pack_path(dir, &pack.name)?;
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create the folder {}", dir.display()))?;
    let text = serde_json::to_string_pretty(pack)?;
    let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("Could not write the pack {}", path.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("Could not write the pack {}", path.display()))?;
    Ok(path)
}

pub fn load_pack(dir: &Path, name: &str) -> Result<ReportPack> {
    let path = pack_path(dir, name)?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("Could not read the pack {}. Packs are in {}.", path.display(), dir.display()))?;
    let mut p: ReportPack = serde_json::from_str(&text).map_err(|e| anyhow!("The pack {} is not valid: {}.", path.display(), e))?;
    p.name = pack_stem(name)?;
    Ok(p)
}

pub fn list_packs(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(str::to_string)).collect()).unwrap_or_default();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

/// The characters Excel does not allow in a sheet name.
const BAD_SHEET_CHARS: [char; 7] = ['[', ']', ':', '*', '?', '/', '\\'];

/// The source names an item reads (its slots).
pub fn item_slots(item: &PackItem) -> Vec<String> {
    match &item.source {
        PackItemKind::Steps { slot } | PackItemKind::Pivot { slot, .. } => vec![slot.clone()],
        PackItemKind::Run { run } => run.source_names(),
    }
}

/// What is wrong with the pack as it is, in words for the person who builds it.
pub fn validate_pack(p: &ReportPack) -> Result<()> {
    if p.items.is_empty() {
        bail!("The pack '{}' has no items. Add an item from a result tab, or a slot's table.", p.name);
    }
    let mut seen: Vec<String> = Vec::new();
    for i in &p.inputs {
        let n = i.slot.trim();
        if n.is_empty() {
            bail!("An input slot has no name. Give each slot a name (for example Loans).");
        }
        if seen.iter().any(|s| s.eq_ignore_ascii_case(n)) {
            bail!("Two input slots are called '{}'. Slot names must be different.", n);
        }
        seen.push(n.to_string());
    }
    let mut sheets: Vec<String> = vec![COVER_SHEET.to_lowercase(), "export_errors".to_string()];
    for it in &p.items {
        let label = if it.title.trim().is_empty() { it.sheet.clone() } else { it.title.clone() };
        let sh = it.sheet.trim();
        if sh.is_empty() {
            bail!("The item '{}' has no sheet name.", label);
        }
        if sh.chars().count() > 31 || sh.contains(BAD_SHEET_CHARS) || sh.starts_with('\'') || sh.ends_with('\'') {
            bail!("The sheet name '{}' of the item '{}' cannot be an Excel sheet name: at most 31 characters, none of [ ] : * ? / \\.", sh, label);
        }
        if sheets.contains(&sh.to_lowercase()) {
            bail!("The sheet name '{}' is used twice (or is a name the workbook keeps: Cover, Export_Errors). Rename the item '{}'.", sh, label);
        }
        sheets.push(sh.to_lowercase());
        for s in item_slots(it) {
            if !seen.iter().any(|x| x.eq_ignore_ascii_case(s.trim())) {
                bail!("The item '{}' reads the slot '{}', which the pack does not have. Add the slot first.", label, s);
            }
        }
    }
    Ok(())
}

// ----------------------------------------------------------------------------------- the run

/// What the person gives a run: a file for each slot and the cutoff date.
#[derive(Clone, Debug, Default)]
pub struct PackRunSpec {
    pub files: BTreeMap<String, PathBuf>,
    pub cutoff: String,
}

/// One input as it was read.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotInfo {
    pub slot: String,
    pub file_name: String,
    pub rows: usize,
    pub hash: String,
}

/// One sheet of the workbook.
#[derive(Clone, Debug)]
pub struct PackSheet {
    pub sheet: String,
    pub title: String,
    pub df: DataFrame,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
}

/// A finished run: the cover, the sheets and what was read. Nothing is written yet.
#[derive(Clone, Debug)]
pub struct PackResult {
    pub pack: String,
    pub cover: DataFrame,
    pub sheets: Vec<PackSheet>,
    pub inputs: Vec<SlotInfo>,
    pub used: Vec<crate::rules::UsedFile>,
    pub output: PackOutputCopy,
}

/// The output choices of the pack the run used.
#[derive(Clone, Debug)]
pub struct PackOutputCopy {
    pub pdf: bool,
    pub scale: NumberDisplayFormat,
    pub decimals: usize,
}

struct SlotData {
    source: TabSource,
    steps: Vec<TransformStep>,
}

struct PackEnv<'a> {
    slots: &'a BTreeMap<String, SlotData>,
    paths: &'a PortablePaths,
}

impl RunEnv for PackEnv<'_> {
    fn tab(&self, name: &str) -> Result<RunTab> {
        let key = self.slots.keys().find(|k| k.eq_ignore_ascii_case(name.trim())).ok_or_else(|| anyhow!("The pack has no input slot '{}'.", name))?;
        let d = &self.slots[key];
        Ok(RunTab { source: d.source.clone(), steps: d.steps.clone() })
    }
    fn name_of(&self, _source: &TabSource, _steps: &[TransformStep]) -> Result<String> {
        bail!("A pack names its sources by slot.")
    }
    fn paths(&self) -> &PortablePaths {
        self.paths
    }
}

/// The blake3 hash of a file, as 64 hex characters.
pub fn file_hash(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).with_context(|| format!("Could not open {}", path.display()))?;
    let mut h = blake3::Hasher::new();
    std::io::copy(&mut f, &mut h).with_context(|| format!("Could not read {}", path.display()))?;
    Ok(h.finalize().to_hex().to_string())
}

fn key_of(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Reads one slot's file: schema, required columns, steps, checks. Returns what the tabs of the pack read from.
fn load_slot(input: &PackInput, file: &Path, paths: &PortablePaths, ctx: &JobCtx) -> Result<(SlotData, SlotInfo, Vec<String>)> {
    let slot = input.slot.trim();
    if !file.is_file() {
        bail!("Slot '{}': the file {} is not there. Choose the file of this month.", slot, file.display());
    }
    let is_parquet = file.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("parquet"));
    let detected = detect_file_schema(file).map_err(|e| anyhow!("Slot '{}': the file {} could not be read: {:#}", slot, file.display(), e))?;
    let headers: Vec<String> = detected.columns.iter().map(|c| c.name.clone()).collect();
    // the schema of the slot: a saved one by name, else the one captured with the slot
    let schema: Option<IngestionConfig> = match (&input.schema_template, &input.schema) {
        (Some(name), _) => {
            let templates = crate::schema_templates::load_templates(&paths.schemas_dir);
            let t = templates.into_iter().find(|t| t.name == *name).ok_or_else(|| anyhow!("Slot '{}': the saved schema '{}' is not in {}. Save it there again, or choose another schema for the slot.", slot, name, paths.schemas_dir.display()))?;
            Some(t.config)
        }
        (None, Some(c)) => Some(c.clone()),
        (None, None) => None,
    };
    let mut required: Vec<String> = input.required_columns.clone();
    if let Some(s) = &schema {
        required.extend(s.columns.iter().map(|c| c.name.clone()));
    }
    let have: Vec<String> = headers.iter().map(|h| key_of(h)).collect();
    let mut missing: Vec<String> = Vec::new();
    for r in &required {
        if !have.contains(&key_of(r)) && !missing.iter().any(|m| key_of(m) == key_of(r)) {
            missing.push(r.clone());
        }
    }
    if !missing.is_empty() {
        bail!("Slot '{}': the file {} lacks the column(s) {}. It has: {}.", slot, file.display(), missing.join(", "), headers.join(", "));
    }
    let config = match &schema {
        Some(s) => crate::schema_templates::apply_template(&detected, s),
        None if is_parquet => IngestionConfig::default(),
        None => detected,
    };
    let problems = config.rename_problems();
    if let Some((i, why)) = problems.first() {
        bail!("Slot '{}': the schema's column {} is not usable: {}.", slot, i + 1, why);
    }
    let source = TabSource::SingleFile { path: file.to_path_buf(), is_parquet, config };
    ctx.check()?;
    ensure_pii_key_for(&source)?;
    let failures = source_parse_failures(&source, &paths.temp_dir)?;
    if !failures.is_empty() {
        let list: Vec<String> = failures.iter().map(|f| format!("'{}' has {} (for example {})", f.column, f.count, f.examples.join(" | "))).collect();
        bail!("Slot '{}': values that could not be converted to the column's type: {}. Fix the file or the slot's schema; the pack does not leave them out.", slot, list.join("; "));
    }
    let mut notes: Vec<String> = Vec::new();
    if let Some(w) = source_row_check(&source, &paths.temp_dir)?.and_then(|c| c.warning()) {
        notes.push(format!("{}: {}", slot, w));
    }
    notes.extend(blank_balance_notes(&source, &input.steps, &paths.temp_dir)?.into_iter().map(|n| format!("{}: {}", slot, n)));
    notes.extend(weighted_mixed_sign_notes(&source, &input.steps, &paths.temp_dir)?.into_iter().map(|n| format!("{}: {}", slot, n)));
    let lf = compile_tab_lazy(&source, &input.steps, &paths.temp_dir).map_err(|e| anyhow!("Slot '{}': its steps could not be run on {}: {:#}", slot, file.display(), e))?;
    let rows = lf.select([len().alias("n")]).collect()?.column("n").ok().and_then(|c| c.cast(&DataType::Int64).ok()).and_then(|c| c.i64().ok().and_then(|v| v.get(0))).unwrap_or(0).max(0) as usize;
    let info = SlotInfo { slot: slot.to_string(), file_name: file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), rows, hash: file_hash(file)? };
    Ok((SlotData { source, steps: input.steps.clone() }, info, notes))
}

fn cover_frame(rows: Vec<[String; 6]>) -> Result<DataFrame> {
    let cols: Vec<Series> = (0..6).map(|c| Series::new(COVER_COLUMNS[c].into(), rows.iter().map(|r| r[c].clone()).collect::<Vec<String>>())).collect();
    Ok(DataFrame::new(cols)?)
}

/// Runs the pack on the given files and cutoff. Any problem stops the run with the slot or item and
/// the reason; the result holds everything the workbook needs, and nothing has been written.
/// `run_time` is the text shown on the cover (the only thing that differs between two runs).
pub fn run_pack(pack: &ReportPack, spec: &PackRunSpec, paths: &PortablePaths, run_time: &str, ctx: &JobCtx) -> Result<PackResult> {
    validate_pack(pack)?;
    let cutoff = spec.cutoff.trim();
    if chrono::NaiveDate::parse_from_str(cutoff, "%Y-%m-%d").is_err() {
        bail!("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).");
    }
    // every slot needs a file, and a file for a slot the pack does not have is a typing mistake
    for f in spec.files.keys() {
        if !pack.inputs.iter().any(|i| key_of(&i.slot) == key_of(f)) {
            bail!("The pack '{}' has no slot called '{}'. Its slots are: {}.", pack.name, f, pack.inputs.iter().map(|i| i.slot.as_str()).collect::<Vec<_>>().join(", "));
        }
    }
    for i in &pack.inputs {
        if !spec.files.keys().any(|f| key_of(f) == key_of(&i.slot)) {
            bail!("The pack '{}' needs a file for the slot '{}'. Choose it (headless: --slot {}=<file>).", pack.name, i.slot, i.slot);
        }
    }
    let total = pack.inputs.len() + pack.items.len();
    let mut slots: BTreeMap<String, SlotData> = BTreeMap::new();
    let mut infos: Vec<SlotInfo> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut used: Vec<crate::rules::UsedFile> = Vec::new();
    for (n, input) in pack.inputs.iter().enumerate() {
        ctx.check()?;
        ctx.set_progress(format!("Reading {}", input.slot), Some(n as f32 / total as f32));
        let file = spec.files.iter().find(|(k, _)| key_of(k) == key_of(&input.slot)).map(|(_, v)| v.clone()).ok_or_else(|| anyhow!("The slot '{}' has no file.", input.slot))?;
        let (res, files) = crate::rules::track_used_files(|| load_slot(input, &file, paths, ctx));
        let (data, info, n_notes) = res?;
        for f in files {
            if !used.contains(&f) {
                used.push(f);
            }
        }
        notes.extend(n_notes);
        slots.insert(input.slot.trim().to_string(), data);
        infos.push(info);
    }
    let env = PackEnv { slots: &slots, paths };
    let mut sheets: Vec<PackSheet> = Vec::new();
    for (n, item) in pack.items.iter().enumerate() {
        ctx.check()?;
        ctx.set_progress(format!("Item {} of {}: {}", n + 1, pack.items.len(), item.title), Some((pack.inputs.len() + n) as f32 / total as f32));
        let fail = |e: anyhow::Error| anyhow!("The item '{}' (sheet '{}') failed: {:#}", item.title, item.sheet, e);
        let (res, files) = crate::rules::track_used_files(|| -> Result<DataFrame> {
            let (source, steps) = match &item.source {
                PackItemKind::Steps { slot } => {
                    let t = env.tab(slot)?;
                    (t.source, t.steps)
                }
                PackItemKind::Pivot { slot, pivot } => {
                    let t = env.tab(slot)?;
                    let step = pivot.clone().ok_or_else(|| anyhow!("the pivot has no pivot step"))?;
                    let mut steps = t.steps;
                    steps.push(step);
                    (t.source, steps)
                }
                PackItemKind::Run { run } => {
                    let mut run = run.clone();
                    run.set_cutoff(cutoff);
                    let mut built = run.build(&env)?;
                    if built.is_empty() {
                        bail!("the saved run makes no table");
                    }
                    let (_, source, steps) = built.remove(0);
                    (source, steps)
                }
            };
            Ok(compile_tab_lazy(&source, &steps, &paths.temp_dir).and_then(|lf| Ok(lf.collect()?))?)
        });
        let df = res.map_err(fail)?;
        for f in files {
            if !used.contains(&f) {
                used.push(f);
            }
        }
        sheets.push(PackSheet { sheet: item.sheet.trim().to_string(), title: item.title.clone(), df, col_formats: item.col_formats.clone() });
    }
    ctx.set_progress("Writing the cover", Some(1.0));

    // the cover: pack, run time, version, cutoff, inputs, sheets, files used
    let mut rows: Vec<[String; 6]> = Vec::new();
    let blank = String::new;
    let r = |a: &str, b: &str, c: &str| [a.to_string(), b.to_string(), c.to_string(), blank(), blank(), blank()];
    rows.push(r("Pack", &pack.name, ""));
    rows.push(r("Run time", run_time, ""));
    rows.push(r("App version", env!("CARGO_PKG_VERSION"), ""));
    rows.push(r("Cutoff date", cutoff, ""));
    if used.iter().any(|u| u.sample) {
        rows.push(r("Warning", "Sample parameters", "Some files below are marked sample: confirm their numbers before they go into a return."));
    }
    for i in &infos {
        rows.push([ "Input".to_string(), i.slot.clone(), i.file_name.clone(), format_with_commas(i.rows as f64, 0), i.hash.clone(), blank() ]);
    }
    for s in &sheets {
        rows.push(r("Sheet", &s.sheet, &s.title));
    }
    for u in &used {
        let mut detail = u.file.clone();
        if !u.as_of.is_empty() {
            detail.push_str(&format!(", as of {}", u.as_of));
        }
        if !u.source.is_empty() {
            detail.push_str(&format!(", source: {}", u.source));
        }
        rows.push([ "Parameter file".to_string(), u.kind.clone(), detail, blank(), blank(), if u.sample { "Yes".to_string() } else { "No".to_string() } ]);
    }
    for n in &notes {
        rows.push(r("Note", "", n));
    }
    Ok(PackResult { pack: pack.name.clone(), cover: cover_frame(rows)?, sheets, inputs: infos, used, output: PackOutputCopy { pdf: pack.output.pdf, scale: pack.output.scale, decimals: pack.output.decimals } })
}

/// Writes the workbook (and the PDF when the pack asks for one). Each file is written to a
/// temporary name and renamed, so a failure leaves no partial file.
pub fn write_pack(result: &PackResult, xlsx: &Path, pdf: Option<&Path>, ctx: &JobCtx) -> Result<String> {
    let mut tabs: Vec<XlsxTab> = vec![XlsxTab { title: COVER_SHEET.to_string(), df: result.cover.clone(), col_formats: HashMap::new(), summary: Vec::new() }];
    tabs.extend(result.sheets.iter().map(|s| XlsxTab { title: s.sheet.clone(), df: s.df.clone(), col_formats: s.col_formats.clone(), summary: Vec::new() }));
    let scale = XlsxScale { num_format: result.output.scale, decimals: result.output.decimals };
    // both files are written to temporary names first and renamed only when both are finished: a
    // PDF that fails leaves no workbook behind either
    ctx.check()?;
    let xlsx_tmp = crate::jobs::TempOutput::new(xlsx);
    let rep = write_xlsx_tabs_noted(tabs, Vec::new(), Vec::new(), xlsx_tmp.path(), EXCEL_MAX_DATA_ROWS, Some(scale))?;
    let mut msg = format!("Wrote {} ({})", xlsx.display(), rep.summary());
    let mut pdf_tmp = None;
    if let Some(p) = pdf {
        let mut ptabs: Vec<PdfTab> = vec![PdfTab { title: COVER_SHEET.to_string(), df: result.cover.clone(), col_formats: HashMap::new(), summary: Vec::new() }];
        ptabs.extend(result.sheets.iter().map(|s| PdfTab { title: s.title.clone(), df: s.df.clone(), col_formats: s.col_formats.clone(), summary: Vec::new() }));
        let tmp = crate::jobs::TempOutput::new(p);
        let prep = crate::exporters::export_pdf_tabs(ptabs, tmp.path(), result.output.scale, result.output.decimals)?;
        msg.push_str(&format!("; wrote {}", p.display()));
        if !prep.truncated_tabs.is_empty() {
            msg.push_str(&format!(" (long tabs cut in the PDF: {})", prep.truncated_tabs.join(", ")));
        }
        pdf_tmp = Some(tmp);
    }
    ctx.check()?;
    xlsx_tmp.commit()?;
    if let Some(t) = pdf_tmp {
        t.commit()?;
    }
    Ok(msg)
}

/// The run time as the cover shows it.
pub fn run_time_now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
