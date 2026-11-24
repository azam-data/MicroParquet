//! The work behind the UI's jobs (Phase 3 R2): opening a tab, refreshing its page and totals,
//! applying a schema, and the exports. These are plain functions of their inputs, so the worker
//! thread, headless mode and the tests all run the same code.

use crate::batch::BatchReport;
use crate::engine::{
    blank_balance_notes, compile_tab_lazy, control_totals_state, control_totals_state_for, remember_baseline,
    source_baseline_sum,
    ensure_pii_key_for, export_to_csv, export_to_parquet, fetch_viewport_slice, validate_steps, ExportMode,
    ExportPath, StepIssue,
    join_status, source_parse_failures, source_row_check, text_columns_that_look_numeric,
    weighted_mixed_sign_notes, write_parse_failures_csv, NumericLookingColumn,
};
use crate::exporters::{
    export_pdf_tabs, export_selected_tabs_to_xlsx_scaled, PdfOptions, split_and_export_by_column_ctx, PdfTab,
    ScaledTabToExport, XlsxScale, PDF_MAX_ROWS_PER_TAB,
};
use crate::jobs::{write_atomic, JobCtx};
use crate::models::{
    ColumnFormatSpec, ControlTotals, IngestionConfig, JoinStatus, NumberDisplayFormat,
    ParquetDialect, ParseFailure, RowCountCheck, SplitExportFormat, SummarySpec, TabSource, TabState,
    TransformStep,
};
use anyhow::{Context, Result};
use polars::prelude::{DataFrame, IntoLazy};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What a finished job hands back to the UI.
pub enum JobOutput {
    ColumnStats(Box<ColumnStatsResult>),
    PivotPreview(Box<PivotPreviewResult>),
    PivotValues(Box<PivotValuesResult>),
    /// The labels of a field for its auto order (Phase 8 W0).
    AutoOrder(Box<PivotValuesResult>),
    StepCounts(Box<StepCountsResult>),
    TextCheck(Box<TextCheckResult>),
    Summary(Box<SummaryResult>),
    Loaded(Box<LoadedTab>),
    Refreshed(Box<Refreshed>),
    Schema(Box<SchemaApplied>),
    SqlChecked(Box<SqlChecked>),
    Chart(Box<ChartResult>),
    Picker(Box<PickerResult>),
    Dq(Box<DqJobResult>),
    Exported(ExportOutcome),
    Batch(Box<BatchReport>),
    /// A tab of a project that was opened (Phase 8 W4).
    ProjectTab(Box<ProjectLoaded>),
    /// The files of a project that changed since it was saved, and the hashes worked out (Phase 8 W4).
    ProjectCheck(Box<ProjectChecked>),
    /// The project built from the open tabs, and the hashes worked out on the way.
    ProjectSave(Box<ProjectBuilt>),
    /// A linked chart tab's new numbers and table (Phase 8 W5).
    ChartTab(Box<ChartTabCalc>),
    /// The tab behind a cell of a pivot (Phase 8 W6).
    ShowRows(Box<ShowRowsResult>),
    /// The values a chart's tick list lists (Phase 8 W10).
    ChartPicker(Box<PickerResult>),
    /// The Nested view of a pivot tab (Phase 9 P2).
    NestedView(Box<NestedViewResult>),
    /// The key check of a link (Phase 9 M0).
    ModelCheck(Box<ModelCheckResult>),
    /// A tab of the rows a key check lists (Phase 9 M0).
    ModelRows(Box<ModelRowsResult>),
    /// The snapshot dates of a date-linked tab (Phase 9 M2).
    DateInfo(Box<DateInfoResult>),
}

/// A tab of a project, built on the worker.
pub struct ProjectLoaded {
    pub loaded: LoadedTab,
    /// The chart's numbers, for a chart tab.
    pub chart: Option<crate::chart::ChartData>,
    /// The tab's place in the project's tab list.
    pub index: usize,
}

/// A linked chart tab to calculate again from its source tab.
pub struct ChartTabRequest {
    pub chart_tab: u64,
    /// What the source tab looked like (`source_signature`) when the request was made.
    pub sig: String,
    pub base: TabSource,
    pub steps: Vec<TransformStep>,
    pub spec: crate::chart::ChartSpec,
    pub temp_dir: PathBuf,
}

pub struct ChartTabCalc {
    pub chart_tab: u64,
    pub sig: String,
    pub data: crate::chart::ChartData,
    /// The chart's table, a Parquet file the program made.
    pub source: TabSource,
}

/// The chart over the source tab's table, and the table of its numbers (what the chart tab shows and exports).
pub fn chart_tab_job(req: ChartTabRequest, ctx: &JobCtx) -> Result<JobOutput> {
    ctx.set_progress("Calculating the chart tab\u{2026}", None);
    ctx.check()?;
    let (data, source) = crate::project::chart_tab_source(&req.base, &req.steps, &req.spec, &req.temp_dir)?;
    Ok(JobOutput::ChartTab(Box::new(ChartTabCalc { chart_tab: req.chart_tab, sig: req.sig, data, source })))
}

/// What a chart tab compares to find out that its source changed: the source, the steps that run and, for a file,
/// its size and time.
pub fn source_signature(source: &TabSource, steps: &[TransformStep]) -> String {
    let stamp = match source {
        TabSource::SingleFile { path, .. } => crate::project::file_stamp(path).map(|(s, m)| format!("{s}|{m}")).unwrap_or_default(),
        _ => String::new(),
    };
    format!("{:?}|{:?}|{}", source, steps, stamp)
}

pub struct ProjectBuilt {
    pub saved: crate::project::SavedProject,
    pub cache: crate::project::HashCache,
    pub path: PathBuf,
}

pub struct ProjectChecked {
    pub warnings: Vec<String>,
    pub hashes: Vec<(PathBuf, u64, i64, String)>,
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
/// The stages a load goes through, shown in the status bar with the time it has taken. A
/// percentage appears only inside a step that reads the file's bytes itself.
pub const LOAD_STAGES: [&str; 3] = ["Reading", "Parsing", "Checking"];

pub fn load_tab(req: LoadRequest, ctx: &JobCtx) -> Result<LoadedTab> {
    let sink_ctx = ctx.clone();
    crate::engine::with_byte_progress(move |what, read, total| sink_ctx.set_bytes(what, read, total), || load_tab_staged(req, ctx))
}

fn load_tab_staged(req: LoadRequest, ctx: &JobCtx) -> Result<LoadedTab> {
    let LoadRequest { tab_id, title, source, steps, col_formats, temp_dir, reconcile_rate_used } = req;
    // 1/3 Reading: open the source, the first page and the control totals.
    ctx.set_stages(&LOAD_STAGES);
    let key_note = ensure_pii_key_for(&source)?;
    ctx.check()?;
    // Steps that cannot run (a column they need is gone) stop the table before them.
    let issues = validate_steps(&source, &steps, &temp_dir).map_err(|e| e.context(STEP_CHECK_FAILED))?;
    let exec_steps = issues.first().map(|i| i.index).unwrap_or(steps.len());
    let lf = compile_tab_lazy(&source, &steps[..exec_steps], &temp_dir)?;
    let (slice, page_error) = match fetch_viewport_slice(lf.clone(), 0, 100) {
        Ok(df) => (df, None),
        Err(e) => (DataFrame::empty(), Some(format!("{:#}", e))),
    };
    let default_ctrl = guess_balance_column(&slice);
    ctx.check()?;
    let totals = control_totals_state_for(Ok(lf.clone()), default_ctrl.as_deref(), &steps[..exec_steps]);
    let (total_rows, control_sum) = (totals.rows.unwrap_or(0), totals.sum);
    ctx.check()?;

    // 2/3 Parsing: the baseline total and the values that could not be converted.
    ctx.set_stage(1);
    // Baseline control sum is always the raw source's (0 steps); it is computed once per
    // unchanged file and reused by every tab on it.
    let baseline_sum = if steps.is_empty() {
        remember_baseline(&source, default_ctrl.as_deref(), control_sum);
        control_sum
    } else {
        source_baseline_sum(&source, default_ctrl.as_deref(), &temp_dir).0
    };
    ctx.check()?;

    let (parse_failures, mut count_note) = match source_parse_failures(&source, &temp_dir) {
        Ok(f) => (f, String::new()),
        Err(e) => (Vec::new(), format!(" ⚠ Could not count parse failures: {:#}", e)),
    };
    ctx.check()?;
    // 3/3 Checking: rows against the file's lines, the join and the balances.
    ctx.set_stage(2);
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
    if let Some(c) = row_check.as_ref() {
        if c.is_explained() {
            // nothing is wrong: the file has line breaks inside quotes
            if let Some(t) = c.status_text() {
                count_note.push_str(&format!(" {}.", t));
            }
        } else if let Some(w) = c.warning() {
            count_note.push_str(&format!(" ⚠ {}", w));
        }
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
    let mut status = match &page_error {
        Some(e) => format!("Tab created, but its first page could not be read: {}.{}", e, count_note),
        None => format!("Tab created and evaluated out-of-core.{}", count_note),
    };
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
        sql_history: Vec::new(),
        sql_error: None,
        parse_failures,
        join_status: join_stat,
        totals,
        row_check,
        summary: Default::default(),
        summary_cells: Vec::new(),
        issues,
        exec_steps,
        history: Default::default(),
        step_counts: Vec::new(),
        step_hashes: Vec::new(),
        column_stats: None,
        page_error,
        summary_error: None,
    };
    Ok(LoadedTab { tab, status })
}

// ----------------------------------------------------------------------------------- refresh

/// Said when the steps could not be checked against the columns they would see.
pub const STEP_CHECK_FAILED: &str = "Could not check the steps against the source's columns (the source could not be read)";

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
    /// Check every step against the columns it would see (false when only the page changed).
    pub validate: bool,
    /// With `validate` false: how many steps run (the last result's).
    pub exec_steps: usize,
}

pub struct Refreshed {
    pub tab_id: u64,
    pub offset: usize,
    /// None when the page could not be read (the tab keeps its last page, marked out of date).
    pub slice: Option<DataFrame>,
    /// Why the page could not be read (the pipeline failed to build, or reading the rows failed).
    pub page_error: Option<String>,
    pub totals: ControlTotals,
    /// `Some` when the baseline was recomputed (inner `None` = no control sum).
    pub baseline: Option<Option<f64>>,
    pub join_status: Option<Option<JoinStatus>>,
    /// The pipeline could not be built or run: the reason, for the status bar.
    pub error: Option<String>,
    /// Status text to show on success (step added + notes).
    pub status: Option<String>,
    /// Steps that cannot run, and how many steps the table ran.
    pub issues: Vec<StepIssue>,
    pub exec_steps: usize,
}

/// Rebuilds a tab's page and totals. A pipeline error is a normal result (`error`), not a failed
/// job, so the tab shows "—" and the reason instead of keeping the previous totals.
pub fn refresh_tab(req: RefreshRequest, ctx: &JobCtx) -> Result<Refreshed> {
    let ctrl = req.control_col.as_deref();
    ctx.check()?;
    // Steps that cannot run stop the table before them; the rest of the list is kept and shown.
    let (issues, exec_steps) = if req.validate {
        let issues = match validate_steps(&req.source, &req.steps, &req.temp_dir) {
            Ok(i) => i,
            Err(e) => {
                // Without the check nobody knows which steps can run: nothing is shown as current.
                let e = e.context(STEP_CHECK_FAILED);
                let msg = format!("{:#}", e);
                return Ok(Refreshed {
                    tab_id: req.tab_id,
                    offset: req.offset,
                    slice: None,
                    page_error: Some(msg.clone()),
                    totals: control_totals_state(Err(e), ctrl),
                    baseline: None,
                    join_status: None,
                    error: Some(msg),
                    status: None,
                    issues: Vec::new(),
                    exec_steps: req.exec_steps.min(req.steps.len()),
                });
            }
        };
        let n = issues.first().map(|i| i.index).unwrap_or(req.steps.len());
        (issues, n)
    } else {
        (Vec::new(), req.exec_steps.min(req.steps.len()))
    };
    let steps = &req.steps[..exec_steps];
    match compile_tab_lazy(&req.source, steps, &req.temp_dir) {
        Ok(lf) => {
            let (slice, page_error) = match fetch_viewport_slice(lf.clone(), req.offset, req.limit) {
                Ok(df) => (Some(df), None),
                Err(e) => (None, Some(format!("{:#}", e))),
            };
            ctx.check()?;
            let totals = control_totals_state_for(Ok(lf), ctrl, steps);
            ctx.check()?;
            let baseline = if req.recalculate_baseline {
                // Baseline of the raw source (0 steps) for the newly selected control column
                Some(source_baseline_sum(&req.source, ctrl, &req.temp_dir).0)
            } else {
                None
            };
            let join = if req.check_join {
                Some(join_status(&req.source, steps, ctrl, &req.temp_dir).ok().flatten())
            } else {
                None
            };
            let status = req
                .added_status
                .as_deref()
                .map(|added| with_balance_notes(added, &req.source, steps, &req.temp_dir));
            Ok(Refreshed {
                tab_id: req.tab_id,
                offset: req.offset,
                slice,
                page_error,
                totals,
                baseline,
                join_status: join,
                error: None,
                status,
                issues,
                exec_steps,
            })
        }
        Err(e) => {
            let msg = format!("{:#}", e);
            Ok(Refreshed {
                tab_id: req.tab_id,
                offset: req.offset,
                slice: None,
                page_error: Some(msg.clone()),
                totals: control_totals_state(Err(e), ctrl),
                baseline: None,
                join_status: None,
                error: Some(msg),
                status: None,
                issues,
                exec_steps,
            })
        }
    }
}

// ------------------------------------------------------------------------------------ value picker

/// The values of a column (as the tab stands before a filter) to calculate on the worker.
pub struct PickerRequest {
    pub tab_id: u64,
    pub token: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub field: String,
    pub search: Option<String>,
    pub temp_dir: PathBuf,
    /// The field is a lookup field (Phase 9 M1): its values are the lookup column's, with "(no match)" and "(blank)".
    pub lookup: Option<crate::model::ModelFieldRef>,
}

pub struct PickerResult {
    pub tab_id: u64,
    pub token: u64,
    pub field: String,
    pub search: Option<String>,
    pub values: std::result::Result<crate::value_picker::PickerValues, String>,
}

pub fn compute_picker_values(req: PickerRequest, ctx: &JobCtx) -> Result<PickerResult> {
    ctx.set_progress("Listing the values…", None);
    ctx.check()?;
    let PickerRequest { tab_id, token, source, steps, field, search, temp_dir, lookup } = req;
    let values = compile_tab_lazy(&source, &steps, &temp_dir)
        .and_then(|lf| match &lookup {
            Some(f) => crate::model::with_lookup_columns(lf, &[crate::model::LookupWant { field: f.clone(), out: field.clone(), as_: crate::model::LookupAs::Text, unmatched_out: None, blank_key_out: None }], "Filter"),
            None => Ok(lf),
        })
        .and_then(|lf| crate::value_picker::picker_values(lf, &field, search.as_deref(), crate::value_picker::PICKER_LIMIT))
        .map_err(|e| format!("{:#}", e));
    Ok(PickerResult { tab_id, token, field, search, values })
}

// ------------------------------------------------------------------------------------ chart

/// The chart of a tab to calculate on the worker (Phase 6 U5).
pub struct ChartRequest {
    pub tab_id: u64,
    /// Which request this is: a result for an older one is dropped.
    pub token: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub spec: crate::chart::ChartSpec,
    pub temp_dir: PathBuf,
}

pub struct ChartResult {
    pub tab_id: u64,
    pub token: u64,
    pub data: std::result::Result<crate::chart::ChartData, String>,
}

pub fn compute_chart_job(req: ChartRequest, ctx: &JobCtx) -> Result<ChartResult> {
    ctx.set_progress("Calculating the chart…", None);
    ctx.check()?;
    let ChartRequest { tab_id, token, source, steps, spec, temp_dir } = req;
    let data = compile_tab_lazy(&source, &steps, &temp_dir)
        .and_then(|lf| crate::charts::calc::compute(crate::pivot_table::without_total_rows(lf, &steps), &spec))
        .map_err(|e| format!("{:#}", e));
    Ok(ChartResult { tab_id, token, data })
}

/// A chart book to calculate and write on the worker (Phase 7 C3): several saved charts of one tab on one HTML page.
pub struct ChartBookRequest {
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub temp_dir: PathBuf,
    /// Name, chart and how its numbers are written, for saved charts that are calculated on the tab.
    pub charts: Vec<(String, crate::chart::ChartSpec, crate::charts::layout::LayoutOpts)>,
    /// Charts already calculated (open chart tabs, the studio chart): written as they are, in this order and
    /// before the saved charts. Nothing is calculated again and no number is scaled a second time.
    pub snapshots: Vec<(String, crate::chart::ChartData, crate::charts::layout::LayoutOpts)>,
    pub title: String,
    pub date: String,
    pub path: PathBuf,
}

/// Calculates each chart over the whole tab and writes the page. One chart that cannot be drawn stops the
/// book: no file is written.
pub fn chart_book_job(req: ChartBookRequest, ctx: &JobCtx) -> Result<JobOutput> {
    use anyhow::Context;
    ctx.set_progress("Calculating the charts\u{2026}", None);
    let ChartBookRequest { source, steps, temp_dir, charts, snapshots, title, date, path } = req;
    let mut datas: Vec<(crate::chart::ChartData, crate::charts::layout::LayoutOpts)> = snapshots.into_iter().map(|(_, d, o)| (d, o)).collect();
    if !charts.is_empty() {
        let lf = compile_tab_lazy(&source, &steps, &temp_dir).map(|lf| crate::pivot_table::without_total_rows(lf, &steps))?;
        for (name, spec, opts) in &charts {
            ctx.check()?;
            let d = crate::charts::calc::compute(lf.clone(), spec).with_context(|| format!("Chart '{}'", name))?;
            datas.push((d, opts.clone()));
        }
    }
    let items: Vec<crate::charts::echarts::BookItem> = datas.iter().map(|(d, o)| crate::charts::echarts::BookItem { data: d, opts: o.clone() }).collect();
    crate::charts::echarts::write_chart_book(&path, &title, &date, &items)?;
    Ok(JobOutput::Exported(ExportOutcome { message: format!("Chart book with {} chart(s) written to {}", items.len(), path.display()), path_used: None, written: Some(path) }))
}

// --------------------------------------------------------------------------- data-quality scorecard

pub struct DqRequest {
    pub tab_id: u64,
    pub token: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub rules: crate::dq::DqRuleSet,
    pub temp_dir: PathBuf,
}

/// The checks of a tab, with the first failing rows of each rule that has some.
#[derive(Clone)]
pub struct DqShown {
    pub result: crate::dq::DqResult,
    /// (rule number, its first failing rows).
    pub previews: Vec<(usize, DataFrame)>,
}

pub struct DqJobResult {
    pub tab_id: u64,
    pub token: u64,
    pub shown: std::result::Result<DqShown, String>,
}

pub fn compute_dq_job(req: DqRequest, ctx: &JobCtx) -> Result<DqJobResult> {
    ctx.set_progress("Checking the data quality\u{2026}", None);
    ctx.check()?;
    let DqRequest { tab_id, token, source, steps, rules, temp_dir } = req;
    let shown = (|| -> anyhow::Result<DqShown> {
        let df = compile_tab_lazy(&source, &steps, &temp_dir)?.collect()?;
        let types = crate::engine::dq_type_stats(&source, &rules, &temp_dir)?;
        let cl_map = crate::banking::ProvisionConfig::load_or_create(&crate::rules::data_dir())?.cl_code_map;
        let result = crate::dq::run_scorecard(&df, &types, &rules, &cl_map)?;
        let mut previews = Vec::new();
        for (i, r) in result.rules.iter().enumerate() {
            if !r.failing_rows.is_empty() {
                previews.push((i, crate::dq::failing_frame(&df, &result, i, Some(crate::dq::FAILING_PREVIEW))?));
            }
        }
        Ok(DqShown { result, previews })
    })()
    .map_err(|e| format!("{:#}", e));
    Ok(DqJobResult { tab_id, token, shown })
}

// ------------------------------------------------------------------------------------ SQL tab

/// A query of the SQL tab to check before it is added as a step (Phase 6 U2).
pub struct SqlCheckRequest {
    pub tab_id: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub query: String,
    /// "New tab" instead of "Apply here".
    pub new_tab: bool,
    pub temp_dir: PathBuf,
}

pub struct SqlChecked {
    pub tab_id: u64,
    pub query: String,
    pub new_tab: bool,
    /// What is wrong with the query, in words; `None` when it runs on the tab's columns.
    pub error: Option<String>,
}

/// Builds the query as the next step of the tab (on the worker) without reading the data. The
/// error is the engine's own message, for example an unknown column.
pub fn check_sql_query(req: SqlCheckRequest, ctx: &JobCtx) -> Result<SqlChecked> {
    let SqlCheckRequest { tab_id, source, steps, query, new_tab, temp_dir } = req;
    ctx.set_progress("Checking the query…", None);
    let n = steps.len();
    let mut all = steps;
    all.push(TransformStep::SqlQuery { query: query.clone() });
    let issues = crate::engine::validate_steps(&source, &all, &temp_dir)?;
    let error = issues.into_iter().find(|i| i.index == n).map(|i| i.reason);
    Ok(SqlChecked { tab_id, query, new_tab, error })
}

// ------------------------------------------------------------------------------------ schema

pub struct SchemaRequest {
    pub tab_id: u64,
    pub new_source: TabSource,
    pub steps: Vec<TransformStep>,
    pub temp_dir: PathBuf,
    /// The steps that are already broken (by index): only a step the new schema breaks is reported.
    pub already_broken: Vec<usize>,
}

pub struct SchemaApplied {
    pub tab_id: u64,
    pub new_source: TabSource,
    pub parse_failures: Vec<ParseFailure>,
    /// Shown when the parse failures could not be counted.
    pub parse_note: Option<String>,
    pub row_check: Option<RowCountCheck>,
    pub status: String,
    /// The tab's steps after the schema: the "Clean line breaks" check box adds or removes the first one.
    pub steps: Vec<TransformStep>,
    /// Steps that worked before and would stop working with the new schema (index, reason), for
    /// example a filter on a column the schema renames. The app asks before applying (Phase 6 U1).
    pub new_breaks: Vec<(usize, String)>,
}

/// Checks that a tab's pipeline still builds with new ingestion settings and recounts its parse
/// failures and rows. The tab is only changed when this succeeds (`Err` = "Schema not applied").
pub fn check_schema_for_tab(req: SchemaRequest, ctx: &JobCtx) -> Result<SchemaApplied> {
    let SchemaRequest { tab_id, new_source, steps, temp_dir, already_broken } = req;
    ctx.set_progress("Checking the new schema…", None);
    let key_note = ensure_pii_key_for(&new_source)?;
    // the file itself must build with the new settings
    let _ = compile_tab_lazy(&new_source, &[], &temp_dir)?;
    ctx.check()?;
    // steps that worked and would stop: the app lists them and asks first. When every step still
    // runs, the whole pipeline is built as a last check.
    let issues = crate::engine::validate_steps(&new_source, &steps, &temp_dir).unwrap_or_default();
    if issues.is_empty() {
        let _ = compile_tab_lazy(&new_source, &steps, &temp_dir)?;
    }
    let new_breaks: Vec<(usize, String)> = issues.into_iter().filter(|i| !already_broken.contains(&i.index)).map(|i| (i.index, i.reason)).collect();
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
    Ok(SchemaApplied { tab_id, new_source, parse_failures, parse_note, row_check, status, steps, new_breaks })
}

// ------------------------------------------------------------------------------------ exports

pub struct ExportOutcome {
    pub message: String,
    /// Parquet and CSV exports: streamed, or collected in memory first.
    pub path_used: Option<ExportPath>,
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
    let how = write_atomic(out_path, ctx, |tmp| export_to_parquet(lf, tmp, dialect, ExportMode::Auto))?;
    let mut message = format!("✅ Exported view to {:?} ({})", out_path, how.note());
    message.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message, path_used: Some(how), written: Some(out_path.to_path_buf()) })
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
    let how = write_atomic(out_path, ctx, |tmp| export_to_csv(lf, tmp, mdy_dates, ExportMode::Auto))?;
    let mut message = format!("✅ Exported view to {:?} ({})", out_path, how.note());
    message.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message, path_used: Some(how), written: Some(out_path.to_path_buf()) })
}

/// A tab as an export job needs it (owned, so it can move to the worker thread).
pub struct ExportTab {
    pub title: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub col_formats: HashMap<String, ColumnFormatSpec>,
    /// The tab's summary choices; Excel and PDF add them as a totals row.
    pub summary: HashMap<String, SummarySpec>,
    /// The tab's pinned control column (weight of rate-like columns by default).
    pub control_col: Option<String>,
    /// A chart tab: its chart (with how its numbers are written); the PDF export draws it instead of the table.
    pub chart: Option<(crate::chart::ChartData, crate::charts::layout::LayoutOpts)>,
}

/// Excel workbook of the ticked tabs. Money columns are written in the chosen scale with the
/// scale in their header ("Balance (BDT M)"); Parquet and CSV exports keep raw values.
/// `totals_row`: add each tab's summary under its data (the export dialog's checkbox).
#[allow(clippy::too_many_arguments)]
pub fn export_tabs_xlsx(
    tabs: &[ExportTab],
    temp_dir: &Path,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    totals_row: bool,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    ctx.set_progress(format!("Writing {}…", display_name(out_path)), None);
    let selected: Vec<ScaledTabToExport> = tabs
        .iter()
        .map(|t| ScaledTabToExport {
            title: t.title.clone(),
            source: &t.source,
            steps: t.steps.as_slice(),
            col_formats: &t.col_formats,
            summary: if totals_row { Some(&t.summary) } else { None },
            control_col: t.control_col.as_deref(),
        })
        .collect();
    let scale = XlsxScale { num_format, decimals: global_decimals };
    let rep = write_atomic(out_path, ctx, |tmp| {
        export_selected_tabs_to_xlsx_scaled(&selected, tmp, temp_dir, scale)
    })?;
    let mut message =
        format!("Exported multi-tab Excel workbook to {:?}: {}", out_path, rep.summary());
    message.push_str(&sidecar_note(out_path, sidecar));
    Ok(ExportOutcome { message, path_used: None, written: Some(out_path.to_path_buf()) })
}

/// The CL-1 summary tab as an Excel sheet laid out like its template (merged group headers, the
/// unit line under the title, the template's cell positions). The tab's table is rebuilt from its
/// source; steps added to the tab afterwards are not part of the sheet.
pub fn export_cl1_xlsx(source: &TabSource, temp_dir: &Path, out_path: &Path, num_format: NumberDisplayFormat, decimals: usize, ctx: &JobCtx) -> Result<ExportOutcome> {
    let TabSource::Cl1Summary { template, .. } = source else {
        anyhow::bail!("Open a CL-1 summary tab first (Banking \u{25b8} Loans & credit \u{25b8} CL-1 summary), then export it.");
    };
    ctx.set_progress(format!("Writing {}\u{2026}", display_name(out_path)), None);
    let df = compile_tab_lazy(source, &[], temp_dir)?.collect()?;
    write_atomic(out_path, ctx, |tmp| crate::cl1::write_cl1_xlsx(&df, template, tmp, num_format, decimals))?;
    let message = format!("Exported the CL-1 summary to {:?} ({}).", out_path, crate::cl1::unit_line(num_format));
    Ok(ExportOutcome { message, path_used: None, written: Some(out_path.to_path_buf()) })
}

/// PDF report of the ticked tabs. `totals_row`: a summary band under each tab's rows.
#[allow(clippy::too_many_arguments)]
pub fn export_tabs_pdf(
    tabs: &[ExportTab],
    temp_dir: &Path,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    totals_row: bool,
    sidecar: Option<&[ParseFailure]>,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    export_tabs_pdf_with(tabs, temp_dir, out_path, num_format, global_decimals, totals_row, sidecar, &PdfOptions::default(), ctx)
}

/// `export_tabs_pdf` with the title and the paper chosen in the export dialog.
#[allow(clippy::too_many_arguments)]
pub fn export_tabs_pdf_with(
    tabs: &[ExportTab],
    temp_dir: &Path,
    out_path: &Path,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    totals_row: bool,
    sidecar: Option<&[ParseFailure]>,
    opts: &PdfOptions,
    ctx: &JobCtx,
) -> Result<ExportOutcome> {
    let mut collected = Vec::new();
    let mut failed = Vec::new();
    // the tabs whose last step is a pivot in the Nested layout: their lines' styles (P2)
    let mut nested_styles: Vec<(String, Vec<crate::pdf_layout::RowStyle>)> = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        ctx.check()?;
        ctx.set_progress(
            format!("Reading '{}' ({} of {})…", tab.title, i + 1, tabs.len()),
            Some(i as f32 / tabs.len().max(1) as f32),
        );
        // a pivot in the Nested layout is printed as its Nested view
        match crate::pivot_view::nested_view_of_tab(&tab.source, &tab.steps, temp_dir).and_then(|v| v.map(|v| crate::pivot_view::pdf_table(&v, PDF_MAX_ROWS_PER_TAB + 1)).transpose()) {
            Ok(Some((df, styles))) => {
                nested_styles.push((tab.title.clone(), styles));
                collected.push(PdfTab { title: tab.title.clone(), df, col_formats: tab.col_formats.clone(), summary: Vec::new(), chart: tab.chart.clone() });
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                failed.push(format!("'{}' ({:#})", tab.title, e));
                continue;
            }
        }
        // The summary is of the whole result; the page shows only the first rows (one past the
        // limit, so the export can say the tab was cut).
        match compile_tab_lazy(&tab.source, &tab.steps, temp_dir).and_then(|lf| {
            // a pivot keeps its own Grand total and gets no extra totals row
            let summary = if totals_row && crate::pivot_table::total_rows_expr(&tab.steps).is_none() {
                summary_of_lazy(lf.clone(), &tab.summary, tab.control_col.as_deref())?
            } else {
                Vec::new()
            };
            let df = lf.limit((PDF_MAX_ROWS_PER_TAB + 1) as u32).collect()?;
            Ok((df, summary))
        }) {
            Ok((df, summary)) => collected.push(PdfTab { title: tab.title.clone(), df, col_formats: tab.col_formats.clone(), summary, chart: tab.chart.clone() }),
            Err(e) => failed.push(format!("'{}' ({:#})", tab.title, e)),
        }
    }
    ctx.set_progress(format!("Writing {}…", display_name(out_path)), None);
    let rep = write_atomic(out_path, ctx, |tmp| {
        crate::exporters::export_pdf_tabs_nested(collected, &nested_styles, tmp, num_format, global_decimals, opts)
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
    Ok(ExportOutcome { message: msg, path_used: None, written: Some(out_path.to_path_buf()) })
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
    Ok(ExportOutcome { message, path_used: None, written: Some(base) })
}

/// The summary cells of a lazy result: the user's choices where there are some, else defaults
/// (rate-like columns weighted by `control_col`).
pub fn summary_of_lazy(
    lf: polars::prelude::LazyFrame,
    overrides: &HashMap<String, SummarySpec>,
    control_col: Option<&str>,
) -> Result<Vec<crate::summary::SummaryCell>> {
    use polars::prelude::*;
    let mut lf = lf;
    let schema = lf.collect_schema()?;
    let cols: Vec<(String, DataType)> = schema.iter().map(|(n, d)| (n.to_string(), d.clone())).collect();
    let specs = crate::summary::effective_specs(&cols, overrides, control_col);
    crate::summary::compute_summary(lf, &specs)
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
    let how = write_atomic(out_path, ctx, |tmp| export_to_parquet(lf, tmp, dialect, ExportMode::Auto))?;
    let orig_mb = csv_path.metadata().map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    let new_mb = out_path.metadata().map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    Ok(ExportOutcome {
        message: format!(
            "✅ Converted to {:?}! ({:.1} MB -> {:.1} MB, {})",
            out_path, orig_mb, new_mb, how.note()
        ),
        path_used: Some(how),
        written: Some(out_path.to_path_buf()),
    })
}

// -------------------------------------------------------------------- Text columns of numbers

pub struct TextCheckRequest {
    pub token: u64,
    /// (tab id, title, source, steps) of every tab the export reads.
    pub tabs: Vec<(u64, String, TabSource, Vec<TransformStep>)>,
    pub temp_dir: PathBuf,
}

pub struct TabTextFindings {
    pub tab_id: u64,
    pub title: String,
    pub columns: Vec<NumericLookingColumn>,
}

pub struct TextCheckResult {
    pub token: u64,
    /// Tabs with at least one Text column that looks like numbers.
    pub findings: Vec<TabTextFindings>,
}

/// Before an export: which Text columns of the exported tabs are mostly numbers.
pub fn check_text_columns(req: TextCheckRequest, ctx: &JobCtx) -> Result<TextCheckResult> {
    let mut findings = Vec::new();
    for (i, (tab_id, title, source, steps)) in req.tabs.iter().enumerate() {
        ctx.check()?;
        ctx.set_progress(
            format!("Checking Text columns of '{}' ({} of {})…", title, i + 1, req.tabs.len()),
            None,
        );
        let lf = compile_tab_lazy(source, steps, &req.temp_dir)?;
        let columns = text_columns_that_look_numeric(lf)?;
        if !columns.is_empty() {
            findings.push(TabTextFindings { tab_id: *tab_id, title: title.clone(), columns });
        }
    }
    Ok(TextCheckResult { token: req.token, findings })
}

// ------------------------------------------------------------------------------ summary row

pub struct SummaryRequest {
    pub tab_id: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub overrides: HashMap<String, SummarySpec>,
    /// The tab's pinned control column (weight of rate-like columns by default).
    pub control_col: Option<String>,
    pub temp_dir: PathBuf,
}

pub struct SummaryResult {
    pub tab_id: u64,
    /// The pipeline the summary is of (`steps_edit::pipeline_hash`); a result for another
    /// pipeline is ignored.
    pub pipeline: u64,
    pub cells: Vec<crate::summary::SummaryCell>,
}

/// The summary row of the whole result after all steps, in one aggregation pass.
pub fn compute_tab_summary(req: SummaryRequest, ctx: &JobCtx) -> Result<SummaryResult> {
    ctx.check()?;
    // a pivot's subtotal and Grand total rows are not data: counting them would add twice
    let lf = crate::pivot_table::without_total_rows(compile_tab_lazy(&req.source, &req.steps, &req.temp_dir)?, &req.steps);
    let cells = summary_of_lazy(lf, &req.overrides, req.control_col.as_deref())?;
    ctx.check()?;
    let pipeline = crate::steps_edit::pipeline_hash(&req.source, &req.steps);
    Ok(SummaryResult { tab_id: req.tab_id, pipeline, cells })
}

// ----------------------------------------------------------------------------- step row counts

pub struct StepCountsRequest {
    pub tab_id: u64,
    pub source: TabSource,
    /// The steps the table runs (those before the first broken one).
    pub steps: Vec<TransformStep>,
    /// Counts already known (same length as `steps` + 1; index 0 = the source). `None` = to do.
    pub known: Vec<Option<usize>>,
    pub hashes: Vec<u64>,
    pub temp_dir: PathBuf,
}

pub struct StepCountsResult {
    pub tab_id: u64,
    pub hashes: Vec<u64>,
    pub counts: Vec<Option<usize>>,
}

/// Rows after each step (index 0 = the source). Only the missing ones are computed; a turned-off
/// step has the count of the step before it.
pub fn compute_step_counts(req: StepCountsRequest, ctx: &JobCtx) -> Result<StepCountsResult> {
    use polars::prelude::*;
    let mut counts = req.known.clone();
    counts.resize(req.steps.len() + 1, None);
    for i in 0..=req.steps.len() {
        ctx.check()?;
        if counts[i].is_some() {
            continue;
        }
        if i > 0 && req.steps[i - 1].is_skipped() {
            counts[i] = counts[i - 1];
            continue;
        }
        ctx.set_progress(format!("Counting rows after step {} of {}…", i, req.steps.len()), Some(i as f32 / (req.steps.len() + 1) as f32));
        let lf = compile_tab_lazy(&req.source, &req.steps[..i], &req.temp_dir)?;
        let df = lf.select([len().alias("n")]).collect()?;
        counts[i] = df.column("n").ok().and_then(|s| s.u32().ok().and_then(|c| c.get(0))).map(|n| n as usize);
    }
    Ok(StepCountsResult { tab_id: req.tab_id, hashes: req.hashes, counts })
}

// -------------------------------------------------------------------------- column inspector

pub struct ColumnStatsRequest {
    pub tab_id: u64,
    pub source: TabSource,
    /// The steps the table runs.
    pub steps: Vec<TransformStep>,
    pub column: String,
    pub temp_dir: PathBuf,
}

pub struct ColumnStatsResult {
    pub tab_id: u64,
    pub stats: crate::column_stats::ColumnStats,
}

/// The fingerprint that ties a statistics result to the pipeline it was computed for.
pub fn stats_key(source: &TabSource, steps: &[TransformStep]) -> u64 {
    crate::steps_edit::pipeline_hash(source, steps)
}

/// Statistics of one column over the whole result.
pub fn compute_tab_column_stats(req: ColumnStatsRequest, ctx: &JobCtx) -> Result<ColumnStatsResult> {
    ctx.check()?;
    ctx.set_progress(format!("Statistics of {}…", req.column), None);
    let key = stats_key(&req.source, &req.steps);
    let lf = crate::pivot_table::without_total_rows(compile_tab_lazy(&req.source, &req.steps, &req.temp_dir)?, &req.steps);
    let stats = crate::column_stats::compute_column_stats(lf, &req.column, key)?;
    Ok(ColumnStatsResult { tab_id: req.tab_id, stats })
}

// -------------------------------------------------------------------------- pivot builder (V1)

/// The kind of a field, for the badge in the pivot builder's field list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
    Date,
    Other,
}

impl FieldKind {
    pub fn of(dt: &polars::prelude::DataType) -> FieldKind {
        use polars::prelude::DataType;
        if dt.is_numeric() {
            FieldKind::Number
        } else if matches!(dt, DataType::Date | DataType::Datetime(_, _)) {
            FieldKind::Date
        } else if dt.is_string() || dt.is_categorical() {
            FieldKind::Text
        } else {
            FieldKind::Other
        }
    }

    /// The badge text: Aa text, # number, D date.
    pub fn badge(&self) -> &'static str {
        match self {
            FieldKind::Text => "Aa",
            FieldKind::Number => "#",
            FieldKind::Date => "D",
            FieldKind::Other => "?",
        }
    }
}

pub struct PivotPreviewRequest {
    pub tab_id: u64,
    /// Identifies the settings this preview is for.
    pub token: u64,
    pub source: TabSource,
    /// The steps the pivot sees.
    pub steps: Vec<TransformStep>,
    pub spec: crate::models::PivotSpec,
    pub temp_dir: PathBuf,
    /// How many rows of the table to bring back (`PIVOT_PREVIEW_ROWS` in the docked dialog, more in the pop-out window).
    pub rows: usize,
}

pub struct PivotPreviewResult {
    pub tab_id: u64,
    pub token: u64,
    /// The fields the pivot can use, with their kinds.
    pub fields: Vec<(String, FieldKind)>,
    /// The first rows of the table, or why there is none.
    pub table: std::result::Result<DataFrame, String>,
    /// All rows of the pivot.
    pub total_rows: usize,
    /// How many rows `table` was cut to.
    pub limit: usize,
    /// The Nested view of the pivot (its first `limit` lines) when the settings choose Nested: built by the same function as the
    /// tab's Nested view, from the same source rows (Phase 9 P2 follow-up).
    pub nested: Option<crate::pivot_view::NestedView>,
}

/// How many rows of the pivot the docked dialog previews (the pop-out window asks for as many as fit, at least this many).
pub const PIVOT_PREVIEW_ROWS: usize = 20;
/// The most rows the pop-out window's preview brings back.
pub const PIVOT_PREVIEW_ROWS_MAX: usize = 500;

/// The field list of the builder, and the pivot's first rows. A pivot that cannot be built gives
/// its message in `table`, not an error of the job (the dialog shows it).
pub fn compute_pivot_preview(req: PivotPreviewRequest, ctx: &JobCtx) -> Result<PivotPreviewResult> {
    ctx.check()?;
    ctx.set_progress("Pivot preview…".to_string(), None);
    let mut lf = compile_tab_lazy(&req.source, &req.steps, &req.temp_dir)?;
    let schema = lf.collect_schema()?;
    let fields: Vec<(String, FieldKind)> = schema.iter().map(|(n, dt)| (n.to_string(), FieldKind::of(dt))).collect();
    ctx.check()?;
    let limit = req.rows.clamp(PIVOT_PREVIEW_ROWS, PIVOT_PREVIEW_ROWS_MAX);
    let mut nested = None;
    let (table, total_rows) = if req.spec.values.is_empty() || (req.spec.rows.is_empty() && req.spec.columns.is_empty()) {
        (Err("Drag fields into Rows or Columns, and a field into Values, to see the table.".to_string()), 0)
    } else {
        let wants_nested = matches!(req.spec.layout, crate::models::PivotLayout::Nested { .. }) && !req.spec.rows.is_empty();
        let lf_nested = if wants_nested { Some(lf.clone()) } else { None };
        match crate::pivot_table::build_pivot(lf, &req.spec) {
            Ok(b) => {
                let n = b.df.height();
                match lf_nested.map(|l| crate::pivot_view::build_nested(l, &req.spec)) {
                    Some(Ok(v)) => {
                        let lines = v.rows.len();
                        nested = Some(v.head(limit));
                        (Ok(b.df.head(Some(limit))), lines)
                    }
                    Some(Err(e)) => (Err(format!("{:#}", e)), 0),
                    None => (Ok(b.df.head(Some(limit))), n),
                }
            }
            Err(e) => (Err(format!("{:#}", e)), 0),
        }
    };
    Ok(PivotPreviewResult { tab_id: req.tab_id, token: req.token, fields, table, total_rows, limit, nested })
}

pub struct PivotValuesRequest {
    pub tab_id: u64,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub field: String,
    pub temp_dir: PathBuf,
    /// The field is a lookup field (Phase 9 M1): its values are the lookup column's, with "(no match)" and "(blank)".
    pub lookup: Option<crate::model::ModelFieldRef>,
}

pub struct PivotValuesResult {
    pub tab_id: u64,
    pub field: String,
    pub values: std::result::Result<(Vec<Option<String>>, bool), String>,
}

/// How many values the Filters checklist lists.
pub const PIVOT_FILTER_VALUES_MAX: usize = 2000;

pub fn compute_pivot_values(req: PivotValuesRequest, ctx: &JobCtx) -> Result<PivotValuesResult> {
    ctx.check()?;
    let lf = compile_tab_lazy(&req.source, &req.steps, &req.temp_dir)?;
    let lf = match &req.lookup {
        Some(f) => match crate::model::with_lookup_columns(lf, &[crate::model::LookupWant { field: f.clone(), out: req.field.clone(), as_: crate::model::LookupAs::Text, unmatched_out: None, blank_key_out: None }], "Filter") {
            Ok(lf) => lf,
            Err(e) => return Ok(PivotValuesResult { tab_id: req.tab_id, field: req.field, values: Err(format!("{:#}", e)) }),
        },
        None => lf,
    };
    let mut values = crate::pivot_table::distinct_field_values(lf, &req.field, PIVOT_FILTER_VALUES_MAX).map_err(|e| format!("{:#}", e));
    if req.lookup.is_some() {
        // the lookup's "(blank)" and "(no match)" come last, in that order
        if let Ok((v, _)) = &mut values {
            v.sort_by_key(crate::model::key_rank);
        }
    }
    Ok(PivotValuesResult { tab_id: req.tab_id, field: req.field, values })
}

/// "Show rows" on a cell of a pivot tab: the tab's steps, the pivot among them, and the cell that was clicked.
pub struct ShowRowsRequest {
    pub base: TabSource,
    /// All the tab's steps. The pivot is `steps[pivot_at]`; steps after it (filter, sort, a new column...) are read through
    /// `pivot_table::pivot_rows_of`.
    pub steps: Vec<TransformStep>,
    pub pivot_at: usize,
    pub spec: crate::models::PivotSpec,
    /// Columns that steps after the pivot added, renamed or rewrote: a cell of those has no rows of its own.
    pub later_made: Vec<String>,
    /// The row of the table on the screen (counted from its first row) and the heading of the column that was clicked.
    pub row: usize,
    pub column: String,
    /// How many rows the table on the screen has, to refuse a pivot that has changed since.
    pub expected_rows: usize,
    pub temp_dir: PathBuf,
}

/// The rows of a cell as a tab: the source and the steps before the pivot, with a filter for the cell.
pub struct ShowRowsResult {
    pub title: String,
    pub base: TabSource,
    pub steps: Vec<TransformStep>,
}

/// "Show rows" on a line of a Nested pivot (Phase 9 P2): the request of a cell, with the Rows values of the line that was clicked
/// (a parent line has the first few, the Grand total none). The Nested layout is only drawn when the pivot is the tab's last step, so
/// `request.row` is not used and no step follows the pivot.
pub struct NestedShowRowsRequest {
    pub request: ShowRowsRequest,
    pub row_key: Vec<Option<String>>,
}

/// Builds the pivot again to find which values the clicked cell stands for, and the tab that holds exactly its rows. When
/// steps follow the pivot, they are applied to the pivot's table and the clicked row is found again in the pivot by its row
/// fields, so a sort or a filter of the result does not move the cell.
pub fn show_rows_job(req: ShowRowsRequest, ctx: &JobCtx) -> Result<JobOutput> {
    show_rows_inner(req, None, ctx)
}

/// `show_rows_job` for a line of the Nested layout: the rows of that line's group (all of them for the Grand total) and column.
pub fn show_rows_nested_job(req: NestedShowRowsRequest, ctx: &JobCtx) -> Result<JobOutput> {
    show_rows_inner(req.request, Some(req.row_key), ctx)
}

fn show_rows_inner(req: ShowRowsRequest, nested_row: Option<Vec<Option<String>>>, ctx: &JobCtx) -> Result<JobOutput> {
    ctx.set_progress("Finding the rows of that cell\u{2026}", None);
    ctx.check()?;
    let Some((before, rest)) = req.steps.split_at_checked(req.pivot_at) else {
        anyhow::bail!("Show rows: the pivot is no longer one of this tab's steps. Open the pivot again.");
    };
    let later = rest.get(1..).unwrap_or(&[]);
    let lf = crate::engine::compile_tab_lazy(&req.base, before, &req.temp_dir)?;
    let build = crate::pivot_table::build_pivot(lf, &req.spec)?;
    ctx.check()?;
    let cell = if let Some(row_key) = nested_row {
        if !later.is_empty() {
            anyhow::bail!("Show rows: steps follow the pivot, so the table on the screen is the flat one. Double-click a number in it.");
        }
        if build.df.height() != req.expected_rows {
            anyhow::bail!("Show rows: the pivot now has {} rows and the table on the screen {}. The data has changed since it was drawn; close the pivot tab and open it again.", build.df.height(), req.expected_rows);
        }
        crate::pivot_table::cell_rows_at(&req.spec, &build, &row_key, &req.column)?
    } else {
        let pivot_row = if later.is_empty() {
            if build.df.height() != req.expected_rows {
                anyhow::bail!("Show rows: the pivot now has {} rows and the table on the screen {}. The data has changed since it was drawn; close the pivot tab and open it again.", build.df.height(), req.expected_rows);
            }
            req.row
        } else {
            if req.later_made.contains(&req.column) || !build.df.get_column_names().iter().any(|n| n.as_str() == req.column.as_str()) {
                anyhow::bail!("Show rows: '{}' was added, renamed or changed by a step after the pivot, so it has no rows of its own. Double-click a number in one of the pivot's own columns.", req.column);
            }
            let shown = crate::engine::apply_steps_to(build.df.clone().lazy(), later)?.collect().context("Show rows could not apply the steps after the pivot")?;
            if shown.height() != req.expected_rows {
                anyhow::bail!("Show rows: the table now has {} rows and the one on the screen {}. The data has changed since it was drawn; close the pivot tab and open it again.", shown.height(), req.expected_rows);
            }
            crate::pivot_table::pivot_row_of_shown(&build, &shown, req.row)?
        };
        crate::pivot_table::cell_rows(&req.spec, &build, pivot_row, &req.column)?
    };
    let mut steps = before.to_vec();
    steps.push(TransformStep::FilterGroup { conditions: cell.conditions, join: crate::models::GroupJoin::And, keep_matching: true });
    Ok(JobOutput::ShowRows(Box::new(ShowRowsResult { title: cell.title, base: req.base, steps })))
}

/// The Nested view of a pivot tab: the tab's steps up to the pivot, which must be the last one run (Phase 9 P2).
pub struct NestedViewRequest {
    pub tab_id: u64,
    pub source: TabSource,
    /// The steps the tab runs (up to `exec_steps`).
    pub steps: Vec<TransformStep>,
    pub temp_dir: PathBuf,
}

pub struct NestedViewResult {
    pub tab_id: u64,
    /// `steps_edit::pipeline_hash` of what the view was built for: a result for steps that have changed since is dropped.
    pub pipeline: u64,
    /// The view, or why it could not be built (the tab then shows the flat table and says why).
    pub view: std::result::Result<Option<crate::pivot_view::NestedView>, String>,
}

/// Builds the Nested view on the worker: the pivot's source rows grouped once for every level, never added up from the flat table.
pub fn compute_nested_view(req: NestedViewRequest, ctx: &JobCtx) -> Result<NestedViewResult> {
    ctx.check()?;
    ctx.set_progress("Nested pivot\u{2026}", None);
    let pipeline = crate::steps_edit::pipeline_hash(&req.source, &req.steps);
    let view = crate::pivot_view::nested_view_of_tab(&req.source, &req.steps, &req.temp_dir).map_err(|e| format!("{:#}", e));
    Ok(NestedViewResult { tab_id: req.tab_id, pipeline, view })
}

/// The key check of one link, with the fingerprint of what it was run for: a result for tabs that have changed since is dropped.
pub struct ModelCheckRequest {
    pub link: crate::model::Link,
    pub data: crate::model::ModelTab,
    pub lookup: crate::model::ModelTab,
    pub fingerprint: u64,
    pub temp_dir: PathBuf,
}

pub struct ModelCheckResult {
    pub link: crate::model::Link,
    pub fingerprint: u64,
    /// The check, or why it could not run.
    pub check: std::result::Result<crate::model::KeyCheck, String>,
}

pub fn compute_model_check(req: ModelCheckRequest, ctx: &JobCtx) -> Result<ModelCheckResult> {
    ctx.check()?;
    ctx.set_progress("Checking the keys\u{2026}", None);
    let check = crate::model::check_link(&req.link, &req.data, &req.lookup, &req.temp_dir, &|| ctx.check()).map_err(|e| {
        if e.downcast_ref::<crate::jobs::Cancelled>().is_some() {
            return "cancelled".to_string();
        }
        format!("{:#}", e)
    });
    if check.as_ref().err().is_some_and(|e| e == "cancelled") {
        ctx.check()?;
    }
    Ok(ModelCheckResult { link: req.link, fingerprint: req.fingerprint, check })
}

/// The snapshot dates of a date-linked tab: how many rows each distinct date has, after the tab's steps (Phase 9 M2).
pub struct DateInfoRequest {
    pub tab_key: String,
    pub column: String,
    pub source: TabSource,
    pub steps: Vec<TransformStep>,
    pub fingerprint: u64,
    pub temp_dir: PathBuf,
}

pub struct DateInfoResult {
    pub tab_key: String,
    pub column: String,
    pub fingerprint: u64,
    pub counts: std::result::Result<Vec<(chrono::NaiveDate, usize)>, String>,
}

pub fn compute_date_info(req: DateInfoRequest, ctx: &JobCtx) -> Result<DateInfoResult> {
    ctx.check()?;
    ctx.set_progress("Reading the snapshot dates\u{2026}", None);
    let counts = compile_tab_lazy(&req.source, &req.steps, &req.temp_dir).and_then(|lf| crate::model_calendar::snapshot_counts(lf, &req.column)).map_err(|e| format!("{:#}", e));
    ctx.check()?;
    Ok(DateInfoResult { tab_key: req.tab_key, column: req.column, fingerprint: req.fingerprint, counts })
}

/// Which list of rows a key check opens as a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelList {
    /// The lookup rows whose key repeats, with a Times column.
    Duplicates,
    /// The data rows whose key has no lookup row.
    Unmatched,
}

pub struct ModelRowsRequest {
    pub list: ModelList,
    pub link: crate::model::Link,
    pub data: crate::model::ModelTab,
    pub lookup: crate::model::ModelTab,
    pub temp_dir: PathBuf,
    pub file_stamp: u64,
}

pub struct ModelRowsResult {
    pub title: String,
    pub source: TabSource,
    pub rows: usize,
}

/// Lists the rows and writes them to a Parquet file in the temporary folder: the tab reads that file (it is a list to send back
/// and fix at source, so it is not saved in a project).
pub fn compute_model_rows(req: ModelRowsRequest, ctx: &JobCtx) -> Result<ModelRowsResult> {
    ctx.check()?;
    ctx.set_progress("Listing the rows\u{2026}", None);
    let (mut df, title, stem) = match req.list {
        ModelList::Duplicates => (crate::model::duplicate_rows(&req.link, &req.lookup, &req.temp_dir)?, format!("Duplicate keys: {}", req.lookup.title), "model_duplicates"),
        ModelList::Unmatched => (crate::model::unmatched_rows(&req.link, &req.data, &req.lookup, &req.temp_dir)?, format!("Unmatched rows: {}", crate::model::link_name(&req.data.title, &req.lookup.title)), "model_unmatched"),
    };
    ctx.check()?;
    std::fs::create_dir_all(&req.temp_dir).with_context(|| "The temporary folder could not be made")?;
    let path = req.temp_dir.join(format!("{}_{}_{}.parquet", stem, std::process::id(), req.file_stamp));
    let f = std::fs::File::create(&path).with_context(|| format!("The list could not be written to {}", path.display()))?;
    polars::prelude::ParquetWriter::new(f).finish(&mut df).context("The list could not be written")?;
    Ok(ModelRowsResult { title, source: TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() }, rows: df.height() })
}
