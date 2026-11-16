//! The work behind the UI's jobs (Phase 3 R2): opening a tab, refreshing its page and totals,
//! applying a schema, and the exports. These are plain functions of their inputs, so the worker
//! thread, headless mode and the tests all run the same code.

use crate::batch::BatchReport;
use crate::engine::{
    blank_balance_notes, compile_tab_lazy, compute_control_totals, control_totals_state,
    ensure_pii_key_for, export_to_csv_stream, export_to_parquet_stream, fetch_viewport_slice,
    join_status, source_parse_failures, source_row_check, weighted_mixed_sign_notes,
    write_parse_failures_csv,
};
use crate::exporters::{
    export_selected_tabs_to_xlsx, export_tabs_to_pdf, split_and_export_by_column_ctx, TabToExport,
    PDF_MAX_ROWS_PER_TAB,
};
use crate::jobs::{write_atomic, JobCtx};
use crate::models::{
    ColumnFormatSpec, ControlTotals, IngestionConfig, JoinStatus, NumberDisplayFormat,
    ParquetDialect, ParseFailure, RowCountCheck, SplitExportFormat, TabSource, TabState,
    TransformStep,
};
use anyhow::Result;
use polars::prelude::DataFrame;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What a finished job hands back to the UI.
pub enum JobOutput {
    Loaded(Box<LoadedTab>),
    Refreshed(Box<Refreshed>),
    Schema(Box<SchemaApplied>),
    Exported(ExportOutcome),
    Batch(Box<BatchReport>),
}

/// The first column named "balance", else the first whose name has balance, amount or delta.
pub fn guess_balance_column(df: &DataFrame) -> Option<String> {
    // Prefer exact "balance" column before partial matches
    for col in df.get_columns() {
        if col.name().eq_ignore_ascii_case("balance") {
            return Some(col.name().to_string());
        }
    }
    for col in df.get_columns() {
        let lower = col.name().to_lowercase();
        if lower.contains("balance") || lower.contains("amount") || lower.contains("delta") {
            return Some(col.name().to_string());
        }
    }
    None
}

/// `status` followed by the blank-balance and mixed-sign notes of the tab's banking steps.
pub fn with_balance_notes(
    status: &str,
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
) -> String {
    let notes = blank_balance_notes(source, steps, temp_dir).and_then(|mut n| {
        n.extend(weighted_mixed_sign_notes(source, steps, temp_dir)?);
        Ok(n)
    });
    match notes {
        Ok(notes) if !notes.is_empty() => format!("{} ⚠ {}", status, notes.join("; ")),
        Ok(_) => status.to_string(),
        Err(e) => format!("{} ⚠ Could not count blank balances: {:#}", status, e),
    }
}

// ------------------------------------------------------------------------------------- load

pub struct LoadRequest {
    pub tab_id: u64,
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
    pub temp_dir: PathBuf,
    /// A period reconcile: the status gets the reconcile summary (true = a rate column was used).
    pub reconcile_rate_used: Option<bool>,
}

pub struct LoadedTab {
    pub tab: TabState,
    pub status: String,
}

/// Builds a new tab: pipeline, first page, totals, baseline, parse failures and row check.
pub fn load_tab(req: LoadRequest, ctx: &JobCtx) -> Result<LoadedTab> {
    let LoadRequest { tab_id, title, source, steps, col_formats, temp_dir, reconcile_rate_used } = req;
    ctx.set_progress(format!("Opening {}…", title), None);
    let key_note = ensure_pii_key_for(&source)?;
    ctx.check()?;
    let lf = compile_tab_lazy(&source, &steps, &temp_dir)?;
    let slice = fetch_viewport_slice(lf.clone(), 0, 100).unwrap_or_default();
    let default_ctrl = guess_balance_column(&slice);
    ctx.check()?;
    let totals = control_totals_state(Ok(lf.clone()), default_ctrl.as_deref());
    let (total_rows, control_sum) = (totals.rows.unwrap_or(0), totals.sum);
    ctx.check()?;

    // Baseline control sum is always computed from the raw source (0 steps)
    let baseline_sum = if steps.is_empty() {
        control_sum
    } else {
        compile_tab_lazy(&source, &[], &temp_dir)
            .ok()
            .and_then(|base_lf| compute_control_totals(base_lf, default_ctrl.as_deref()).ok())
            .and_then(|(_, s)| s)
    };
    ctx.check()?;

    let (parse_failures, mut count_note) = match source_parse_failures(&source, &temp_dir) {
        Ok(f) => (f, String::new()),
        Err(e) => (Vec::new(), format!(" ⚠ Could not count parse failures: {:#}", e)),
    };
    ctx.check()?;
    let row_check = match source_row_check(&source, &temp_dir) {
        Ok(c) => c,
        Err(e) => {
            count_note.push_str(&format!(
                " ⚠ Could not compare rows with the file's lines: {:#}",
                e
            ));
            None
        }
    };
    if let Some(w) = row_check.as_ref().and_then(|c| c.warning()) {
        count_note.push_str(&format!(" ⚠ {}", w));
    }
    let join_stat = match join_status(&source, &steps, default_ctrl.as_deref(), &temp_dir) {
        Ok(j) => j,
        Err(e) => {
            count_note.push_str(&format!(
                " ⚠ Could not compare the join with its left table: {:#}",
                e
            ));
            None
        }
    };
    let mut status = format!("Tab created and evaluated out-of-core.{}", count_note);
    if let Some(n) = key_note {
        status.push_str(&format!(" 🔑 {}", n));
    }
    let status = with_balance_notes(&status, &source, &steps, &temp_dir);
    let mut status = status;
    if let Some(rate_used) = reconcile_rate_used {
        match crate::banking::reconcile_status_note(lf, rate_used) {
            Ok(n) => status.push_str(&n),
            Err(e) => status.push_str(&format!(
                " ⚠ Could not summarise the reconcile (keys with several rows, mixed-sign balances): {:#}",
                e
            )),
        }
    }
    let tab = TabState {
        id: tab_id,
        title,
        source,
        steps,
        col_formats,
        total_rows,
        control_col: default_ctrl,
        control_sum,
        baseline_control_sum: baseline_sum,
        viewport_offset: 0,
        slice_offset: 0,
        viewport_limit: 100,
        cached_slice: slice,
        sql_scratchpad: "SELECT * FROM self LIMIT 100".to_string(),
        parse_failures,
        join_status: join_stat,
        totals,
        row_check,
    };
    Ok(LoadedTab { tab, status })
}

// ----------------------------------------------------------------------------------- refresh

pub struct RefreshRequest {
    pub tab_id: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub offset: usize,
    pub limit: usize,
    pub control_col: Option<String>,
    pub recalculate_baseline: bool,
    /// The tab is a join: its status against the left table is recomputed too.
    pub check_join: bool,
    /// "Step added: …" text; when set, the result carries a status with the balance notes.
    pub added_status: Option<String>,
    pub temp_dir: PathBuf,
}

pub struct Refreshed {
    pub tab_id: u64,
    pub offset: usize,
    /// None when the page could not be read (the tab keeps its last page).
    pub slice: Option<DataFrame>,
    pub totals: ControlTotals,
    /// `Some` when the baseline was recomputed (inner `None` = no control sum).
    pub baseline: Option<Option<f64>>,
    pub join_status: Option<Option<JoinStatus>>,
    /// The pipeline could not be built or run: the reason, for the status bar.
    pub error: Option<String>,
    /// Status text to show on success (step added + notes).
    pub status: Option<String>,
}

/// Rebuilds a tab's page and totals. A pipeline error is a normal result (`error`), not a failed
/// job, so the tab shows "—" and the reason instead of keeping the previous totals.
pub fn refresh_tab(req: RefreshRequest, ctx: &JobCtx) -> Result<Refreshed> {
    let ctrl = req.control_col.as_deref();
    ctx.check()?;
    match compile_tab_lazy(&req.source, &req.steps, &req.temp_dir) {
        Ok(lf) => {
            let slice = fetch_viewport_slice(lf.clone(), req.offset, req.limit).ok();
            ctx.check()?;
            let totals = control_totals_state(Ok(lf), ctrl);
            ctx.check()?;
            let baseline = if req.recalculate_baseline {
                // Recompute baseline from the base source (0 steps) for the newly selected control column
                Some(
                    compile_tab_lazy(&req.source, &[], &req.temp_dir)
                        .ok()
                        .and_then(|base_lf| compute_control_totals(base_lf, ctrl).ok())
                        .and_then(|(_, s)| s),
                )
            } else {
                None
            };
            let join = if req.check_join {
                Some(join_status(&req.source, &req.steps, ctrl, &req.temp_dir).ok().flatten())
            } else {
                None
            };
            let status = req
                .added_status
                .as_deref()
                .map(|added| with_balance_notes(added, &req.source, &req.steps, &req.temp_dir));
            Ok(Refreshed {
                tab_id: req.tab_id,
                offset: req.offset,
                slice,
                totals,
                baseline,
                join_status: join,
                error: None,
                status,
            })
        }
        Err(e) => {
            let msg = format!("{:#}", e);
            Ok(Refreshed {
                tab_id: req.tab_id,
                offset: req.offset,
                slice: None,
                totals: control_totals_state(Err(e), ctrl),
                baseline: None,
                join_status: None,
                error: Some(msg),
                status: None,
            })
        }
    }
}

// ------------------------------------------------------------------------------------ schema

pub struct SchemaRequest {
    pub tab_id: u64,
    pub new_source: TabSource,
    pub steps: Vec<TransformStep>,
    pub temp_dir: PathBuf,
}

pub struct SchemaApplied {
    pub tab_id: u64,
    pub new_source: TabSource,
    pub parse_failures: Vec<ParseFailure>,
    /// Shown when the parse failures could not be counted.
    pub parse_note: Option<String>,
    pub row_check: Option<RowCountCheck>,
    pub status: String,
}

/// Checks that a tab's pipeline still builds with new ingestion settings and recounts its parse
/// failures and rows. The tab is only changed when this succeeds (`Err` = "Schema not applied").
pub fn check_schema_for_tab(req: SchemaRequest, ctx: &JobCtx) -> Result<SchemaApplied> {
    let SchemaRequest { tab_id, new_source, steps, temp_dir } = req;
    ctx.set_progress("Checking the new schema…", None);
    let key_note = ensure_pii_key_for(&new_source)?;
    let _ = compile_tab_lazy(&new_source, &steps, &temp_dir)?;
    ctx.check()?;
    let (parse_failures, parse_note) = match source_parse_failures(&new_source, &temp_dir) {
        Ok(f) => (f, None),
        Err(e) => (Vec::new(), Some(format!("⚠ Could not count parse failures: {:#}", e))),
    };
    ctx.check()?;
    let mut row_note = String::new();
    let row_check = match source_row_check(&new_source, &temp_dir) {
        Ok(c) => {
            if let Some(w) = c.as_ref().and_then(|c| c.warning()) {
                row_note = format!(" ⚠ {}", w);
            }
            c
        }
        Err(e) => {
            row_note = format!(" ⚠ Could not compare rows with the file's lines: {:#}", e);
            None
        }
    };
    let mut status = format!("Schema applied and tab reloaded.{}", row_note);
    if let Some(n) = key_note {
        status.push_str(&format!(" 🔑 {}", n));
    }
    Ok(SchemaApplied { tab_id, new_source, parse_failures, parse_note, row_check, status })
}

// ------------------------------------------------------------------------------------ exports

pub struct ExportOutcome {
    pub message: String,
    /// The file (or `<folder>/split_export` base name) that was written.
    pub written: Option<PathBuf>,
}

fn sidecar_note(output: &Path, failures: Option<&[ParseFailure]>) -> String {
    match failures {
        Some(rows) => match write_parse_failures_csv(output, rows) {
            Ok(p) => format!(" Parse failures listed in {:?}.", p),
            Err(e) => format!(" ⚠ {:#}", e),
        },
        None => String::new(),
    }
}

/// Exports a tab's view to Parquet through a temporary name. `sidecar` = parse failures to list
/// next to the file ("Export anyway").
pub fn export_tab_parquet(
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
    out_path: &Path,
    dialect: ParquetDialect,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    ctx.set_progress(format!("Writing {}…", display_name(out_path)), None);
    let lf = compile_tab_lazy(source, steps, temp_dir)?;
    write_atomic(out_path, ctx, |tmp| export_to_parquet_stream(lf, tmp, dialect))?;
    let mut message = format!("✅ Exported view to {:?}", out_path);
    message.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message, written: Some(out_path.to_path_buf()) })
}

pub fn export_tab_csv(
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
    out_path: &Path,
    mdy_dates: bool,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    ctx.set_progress(format!("Writing {}…", display_name(out_path)), None);
    let lf = compile_tab_lazy(source, steps, temp_dir)?;
    write_atomic(out_path, ctx, |tmp| export_to_csv_stream(lf, tmp, mdy_dates))?;
    let mut message = format!("✅ Exported view to {:?}", out_path);
    message.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message, written: Some(out_path.to_path_buf()) })
}

/// A tab as an export job needs it (owned, so it can move to the worker thread).
pub struct ExportTab {
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
}

pub fn export_tabs_xlsx(
    tabs: &[ExportTab],
    temp_dir: &Path,
    out_path: &Path,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    ctx.set_progress(format!("Writing {}…", display_name(out_path)), None);
    let selected: Vec<TabToExport> =
        tabs.iter().map(|t| (t.title.clone(), &t.source, t.steps.as_slice())).collect();
    let rep = write_atomic(out_path, ctx, |tmp| {
        export_selected_tabs_to_xlsx(&selected, tmp, temp_dir)
    })?;
    let mut message =
        format!("Exported multi-tab Excel workbook to {:?}: {}", out_path, rep.summary());
    message.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message, written: Some(out_path.to_path_buf()) })
}

pub fn export_tabs_pdf(
    tabs: &[ExportTab],
    temp_dir: &Path,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    let mut collected = Vec::new();
    let mut failed = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        ctx.check()?;
        ctx.set_progress(
            format!("Reading '{}' ({} of {})…", tab.title, i + 1, tabs.len()),
            Some(i as f32 / tabs.len().max(1) as f32),
        );
        // One row past the limit, so the export can say the tab was cut.
        match compile_tab_lazy(&tab.source, &tab.steps, temp_dir)
            .and_then(|lf| Ok(lf.limit((PDF_MAX_ROWS_PER_TAB + 1) as u32).collect()?))
        {
            Ok(df) => collected.push((tab.title.clone(), df, tab.col_formats.clone())),
            Err(e) => failed.push(format!("'{}' ({:#})", tab.title, e)),
        }
    }
    ctx.set_progress(format!("Writing {}…", display_name(out_path)), None);
    let rep = write_atomic(out_path, ctx, |tmp| {
        export_tabs_to_pdf(collected, tmp, num_format, global_decimals)
    })?;
    let mut msg = format!("Exported multi-tab PDF report to {:?}", out_path);
    let s = rep.summary();
    if !s.is_empty() {
        msg.push_str(&format!(" ⚠ {}", s));
    }
    if !failed.is_empty() {
        msg.push_str(&format!(" ⚠ Not exported (failed): {}", failed.join("; ")));
    }
    msg.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message: msg, written: Some(out_path.to_path_buf()) })
}

#[allow(clippy::too_many_arguments)]
pub fn export_split(
    source: &TabSource,
    steps: &[TransformStep],
    temp_dir: &Path,
    out_dir: &Path,
    split_col: &str,
    format: SplitExportFormat,
    dialect: ParquetDialect,
    mdy_dates: bool,
    control_col: Option<&str>,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    ctx.set_progress("Splitting…", None);
    let lf = compile_tab_lazy(source, steps, temp_dir)?;
    let cnt = split_and_export_by_column_ctx(
        lf, split_col, out_dir, format, dialect, mdy_dates, control_col, ctx,
    )?;
    let base = out_dir.join("split_export");
    let mut message = format!(
        "Split & exported {} individual files into {:?} (rows and control sums in split_manifest.csv)",
        cnt, out_dir
    );
    message.push_str(&sidecar_note(&base, sidecar));
    Ok(ExportOutcome { message, written: Some(base) })
}

fn display_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// Convenience for the schema modal's "Convert directly to Parquet": returns the status text.
pub fn convert_csv_to_parquet(
    csv_path: &Path,
    config: IngestionConfig,
    temp_dir: &Path,
    out_path: &Path,
    dialect: ParquetDialect,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    ctx.set_progress(format!("Converting {}…", display_name(csv_path)), None);
    let src = TabSource::SingleFile { path: csv_path.to_path_buf(), is_parquet: false, config };
    let lf = compile_tab_lazy(&src, &[], temp_dir)?;
    write_atomic(out_path, ctx, |tmp| export_to_parquet_stream(lf, tmp, dialect))?;
    let orig_mb = csv_path.metadata().map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    let new_mb = out_path.metadata().map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    Ok(ExportOutcome {
        message: format!("✅ Converted to {:?}! ({:.1} MB -> {:.1} MB)", out_path, orig_mb, new_mb),
        written: Some(out_path.to_path_buf()),
    })
}
