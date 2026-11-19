//! Saved tool runs in the UI (Phase 6 P1): a result tab's menu gets "Re-run with changes" (the
//! tool's dialog opens with the inputs of the run) and "Save run…" (writes
//! `portable_data/runs/<name>.json`); File ▸ "Saved tool runs" lists the saved runs.

use crate::ui::cashflow_ladder::CfLine;
use crate::ui::gap_ladder::{GapRow, RoleChoice};
use crate::ui::prelude::*;
use micro_parquet::banking::GapRole;
use micro_parquet::tool_runs::{describe, list_runs, load_run, run_file_stem, run_path, runs_dir, save_run, RunEnv, RunTab, SavedRun, ToolRun};

/// The open tabs seen as the sources of a run.
pub struct AppRunEnv<'a> {
    pub tabs: &'a [TabState],
    pub paths: &'a PortablePaths,
}

fn tab_steps(t: &TabState) -> Vec<TransformStep> {
    t.steps[..t.exec_steps.min(t.steps.len())].to_vec()
}

impl RunEnv for AppRunEnv<'_> {
    fn tab(&self, name: &str) -> anyhow::Result<RunTab> {
        let t = tab_by_name(self.tabs, name).ok_or_else(|| anyhow::anyhow!("The tab '{}' is not open. Open it (or the file of that name) first.", name))?;
        Ok(RunTab { source: t.source.clone(), steps: tab_steps(t) })
    }

    fn name_of(&self, source: &TabSource, steps: &[TransformStep]) -> anyhow::Result<String> {
        let key = |s: &TabSource, st: &[TransformStep]| format!("{:?}|{:?}", s, st);
        let want = key(source, steps);
        if let Some(t) = self.tabs.iter().find(|t| key(&t.source, &tab_steps(t)) == want || key(&t.source, &t.steps) == want) {
            return Ok(t.title.clone());
        }
        // a source with no tab open any more: the file it reads
        match source {
            TabSource::SingleFile { path, .. } if steps.is_empty() && path.file_name().is_some() => Ok(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            _ => anyhow::bail!("The tab this result was made from is not open any more. Open it again (with the same steps) and repeat the action."),
        }
    }

    fn paths(&self) -> &PortablePaths {
        self.paths
    }
}

/// The tab called `name`: a tab with that title, else the first tab opened from a file of that name.
pub fn tab_by_name<'a>(tabs: &'a [TabState], name: &str) -> Option<&'a TabState> {
    tabs.iter().find(|t| t.title == name).or_else(|| tabs.iter().find(|t| opened_from_file(t, name)))
}

/// A tab opened straight from the file called `name` (a result of a tool is never "the file").
fn opened_from_file(t: &TabState, name: &str) -> bool {
    matches!(&t.source, TabSource::SingleFile { path, .. } if path.file_name().is_some_and(|n| n.to_string_lossy() == name))
}

/// A tab a run can be saved from: a result of a tool, or a pivot made into a new tab.
pub fn tab_can_be_saved(tab: &TabState) -> bool {
    !matches!(tab.source, TabSource::SingleFile { .. }) || tab.steps.last().is_some_and(|s| matches!(s.inner(), TransformStep::GroupByPivot { .. } | TransformStep::PivotTable { .. }))
}

/// What the two run dialogs drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct RunsRects {
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub rows: Vec<(String, egui::Rect)>,
    pub message: Option<egui::Rect>,
}

/// The run being saved and the name typed for it.
#[derive(Clone, Debug)]
pub struct SaveRunDraft {
    pub run: ToolRun,
    pub name: String,
    pub error: String,
    /// A run of this name exists: the next Save replaces it.
    pub confirm_replace: bool,
}

#[derive(Default)]
pub struct RunsState {
    pub draft: Option<SaveRunDraft>,
    /// The saved runs of the folder: the name and the run (or why it cannot be read).
    pub saved: Vec<(String, Result<SavedRun, String>)>,
    pub rects: RunsRects,
}

impl MicroParquetApp {
    pub(crate) fn run_env(&self) -> AppRunEnv<'_> {
        AppRunEnv { tabs: &self.tabs, paths: &self.paths }
    }

    /// The run behind tab `idx`, or why it has none.
    pub(crate) fn tab_run(&self, idx: usize) -> Result<ToolRun, String> {
        let tab = self.tabs.get(idx).ok_or_else(|| "That tab is not open.".to_string())?;
        if !tab_can_be_saved(tab) {
            return Err("This tab was not made by a tool whose run can be saved.".to_string());
        }
        ToolRun::from_source(&tab.source, &tab.steps, &self.run_env()).map_err(|e| format!("{:#}", e))
    }

    /// "Save run…" of tab `idx`.
    pub(crate) fn open_save_run(&mut self, idx: usize) {
        match self.tab_run(idx) {
            Ok(run) => {
                let name = self.tabs.get(idx).map(|t| t.title.clone()).unwrap_or_default();
                let name = micro_parquet::paths::sanitize_file_name(&name);
                self.runs.draft = Some(SaveRunDraft { run, name, error: String::new(), confirm_replace: false });
                self.active_tool = ActiveToolModal::SaveRun;
            }
            Err(e) => self.status_message = format!("Save run: {}", e),
        }
    }

    /// "Re-run with changes" of tab `idx`: the tool's dialog opens with the run's inputs.
    pub(crate) fn rerun_tab_with_changes(&mut self, idx: usize) {
        match self.tab_run(idx) {
            Ok(run) => {
                if let Err(e) = self.prefill_run(&run) {
                    self.status_message = format!("Re-run: {}", e);
                }
            }
            Err(e) => self.status_message = format!("Re-run: {}", e),
        }
    }

    /// File ▸ "Saved tool runs": reads the folder and opens the list.
    pub(crate) fn open_saved_runs(&mut self) {
        let dir = runs_dir();
        self.runs.saved = list_runs(&dir).into_iter().map(|n| (n.clone(), load_run(&dir, &n).map_err(|e| format!("{:#}", e)))).collect();
        self.active_tool = ActiveToolModal::SavedRuns;
    }

    fn tab_index(&self, name: &str) -> Result<usize, String> {
        if name.trim().is_empty() {
            return Err("The saved run names no source tab for one of its inputs.".to_string());
        }
        self.tabs
            .iter()
            .position(|t| t.title == name)
            .or_else(|| self.tabs.iter().position(|t| opened_from_file(t, name)))
            .ok_or_else(|| format!("The tab '{}' is not open. Open it (or the file of that name) and try again.", name))
    }

    fn tab_indices(&self, names: &[String]) -> Result<Vec<usize>, String> {
        names.iter().map(|n| self.tab_index(n)).collect()
    }

    /// Opens the dialog of the run's tool with the run's inputs in it. Every tab it names must be open.
    pub(crate) fn prefill_run(&mut self, run: &ToolRun) -> Result<(), String> {
        let names = run.source_names();
        let idx = self.tab_indices(&names)?;
        let first = idx.first().copied();
        match run {
            ToolRun::GapLadder(r) => {
                self.active_tab = first.unwrap_or(self.active_tab);
                self.open_gap_ladder();
                match self.gap.presets.iter().position(|p| p.name == r.preset) {
                    Some(i) => self.gap.preset_idx = i,
                    None => self.status_message = format!("The bucket preset '{}' of the saved run is not in the list: choose one.", r.preset),
                }
                self.gap.cutoff = r.cutoff.clone();
                self.gap.null_as_sight = r.null_as_sight;
                self.gap.limits = None;
                self.gap.apply_behaviour = !r.behaviour_file.is_empty();
                if !r.behaviour_file.is_empty() {
                    self.gap.behaviour_sel = r.behaviour_file.clone();
                }
                self.gap.rows = r
                    .inputs
                    .iter()
                    .zip(&idx)
                    .map(|(i, t)| {
                        let (role, side_col) = match &i.spec.role {
                            GapRole::Asset => (RoleChoice::Asset, String::new()),
                            GapRole::Liability => (RoleChoice::Liability, String::new()),
                            GapRole::Side(c) => (RoleChoice::Side, c.clone()),
                        };
                        GapRow { tab: *t, role, side_col, amount_col: i.spec.amount_col.clone(), maturity_col: i.spec.maturity_col.clone(), product_col: i.product_col.clone() }
                    })
                    .collect();
            }
            ToolRun::IrrbbLadder(r) => {
                self.active_tab = first.unwrap_or(self.active_tab);
                self.open_cf_ladder();
                self.cf.cutoff = r.cutoff.clone();
                self.cf.mode = r.mode;
                self.cf.scenario = r.scenario;
                self.cf.blank_on = r.blank_maturity_on;
                self.cf.behaviour_sel = r.behaviour_file.clone();
                self.cf.behaviour = None;
                self.cf.lines = r.inputs.iter().zip(&idx).map(|(i, t)| CfLine::from_spec(*t, &i.spec)).collect();
            }
            ToolRun::IrrbbSix(r) => {
                self.active_tab = first.unwrap_or(self.active_tab);
                self.open_irrbb();
                let g = &mut self.irrbb;
                g.cutoff = r.cutoff.clone();
                g.mode = r.mode;
                g.blank_on = r.blank_maturity_on;
                g.tier1 = format!("{}", r.tier1);
                g.compounding = r.compounding;
                g.post_floor_on = r.post_shock_floor_pct.is_some();
                g.post_floor = r.post_shock_floor_pct.map(|v| format!("{}", v)).unwrap_or_else(|| "0".to_string());
                g.nii_floor_on = r.nii_floor_pct.is_some();
                g.nii_floor = r.nii_floor_pct.map(|v| format!("{}", v)).unwrap_or_else(|| "0".to_string());
                g.behaviour_sel = r.behaviour_file.clone();
                g.behaviour = None;
                g.beta_sel = r.beta_file.clone();
                g.shocks = None;
                g.lines = r.inputs.iter().zip(&idx).map(|(i, t)| CfLine::from_spec(*t, &i.spec)).collect();
            }
            ToolRun::RepricingGap(r) => {
                self.active_tab = idx[0];
                self.init_tool_defaults_from_active_tab();
                let s = &r.spec;
                let step = TransformStep::RateShockSim {
                    bal_col: s.bal_col.clone(),
                    rate_col: s.rate_col.clone(),
                    mat_col: s.mat_col.clone(),
                    cutoff_date: s.cutoff_date.clone(),
                    shock_bps: s.shock_bps,
                    side_col: s.side_col.clone(),
                    reprice_col: s.reprice_col.clone(),
                    beta_file: s.beta_file.clone(),
                    product_col: s.product_col.clone(),
                    floor_pct: s.floor_pct,
                };
                let cols = self.tabs.get(idx[0]).map(|t| t.available_columns()).unwrap_or_default();
                let tool = self.prefill_from_step(&step, &cols).ok_or_else(|| "The rate-shock dialog could not be filled.".to_string())?;
                self.active_tool = tool;
            }
            ToolRun::ClMigration(r) => {
                self.active_tab = idx[1];
                self.open_cl_migration();
                let s = &mut self.cl_migration;
                s.prev_tab = idx[0];
                s.curr_tab = idx[1];
                s.key_col = r.spec.key_col.clone();
                s.bal_col = r.spec.bal_col.clone();
                s.cl_col = r.spec.cl_col.clone();
                s.segment_col = r.spec.segment_col.clone().unwrap_or_default();
                s.measure = r.spec.measure;
                s.pct_of_row = r.spec.pct_of_row;
                s.balance_basis = r.spec.balance_basis;
            }
            ToolRun::MomVariance(r) => {
                self.active_tab = idx[1];
                self.open_mom();
                let s = &mut self.mom;
                s.prev_tab = idx[0];
                s.curr_tab = idx[1];
                s.spec = r.spec.clone();
                s.key1 = r.spec.key_cols.first().cloned().unwrap_or_default();
                s.key2 = r.spec.key_cols.get(1).cloned().unwrap_or_default();
                s.bal = r.spec.bal_col.clone();
                s.rate = r.spec.rate_col.clone().unwrap_or_default();
                s.count = r.spec.count_col.clone().unwrap_or_default();
                s.scale = r.spec.scale;
                s.amount_text = format!("{}", r.spec.material_amount);
                s.pct_text = format!("{}", r.spec.material_pct);
            }
            ToolRun::DqScorecard(r) => {
                self.active_tab = idx[0];
                self.open_dq();
                self.dq.set = r.rules.clone();
            }
            ToolRun::Cl1Summary(r) => {
                self.active_tab = idx[0];
                self.open_cl1_summary();
                let s = &mut self.cl1;
                s.category_by_col = r.spec.category_col.is_some();
                s.category_col = r.spec.category_col.clone().unwrap_or_default();
                if !r.spec.fixed_category.is_empty() {
                    s.fixed_category = r.spec.fixed_category.clone();
                }
                s.grade_col = r.spec.grade_col.clone();
                for (id, c) in &r.spec.measure_cols {
                    if let Some(slot) = s.measure_cols.iter_mut().find(|(i, _)| i == id) {
                        slot.1 = c.clone();
                    }
                }
            }
            ToolRun::Liquidity(r) => {
                self.active_tab = idx[0];
                self.open_liquidity();
                let s = &mut self.liq;
                s.map_sel = r.map_file.clone();
                s.map = None;
                s.gl_col = r.spec.gl_col.clone();
                s.balance_col = r.spec.balance_col.clone();
                s.sign = r.spec.sign;
                s.maturity_col = r.spec.maturity_col.clone().unwrap_or_default();
                s.maturity_unit = r.spec.maturity_unit;
            }
            ToolRun::EclSummary(r) => {
                self.active_tab = idx[0];
                self.open_ifrs9();
                if let Some(step) = &r.ecl_step {
                    let cols = self.tabs.get(idx[0]).map(|t| t.available_columns()).unwrap_or_default();
                    self.prefill_from_step(step, &cols);
                }
                self.ecl.grade_col = r.spec.grade_col.clone();
                self.ecl.provision_col = r.spec.provision_col.clone();
                self.ecl.files = None;
                self.active_tool = ActiveToolModal::IfrsEcl;
            }
            ToolRun::PeriodReconcile(r) => {
                self.active_tab = idx[1];
                let s = &mut self.recon;
                s.prev_tab = idx[0];
                s.curr_tab = idx[1];
                s.key_col = r.key_col.clone();
                s.bal_col = r.bal_col.clone();
                s.rate_col = r.rate_col.clone().unwrap_or_default();
                s.cl_col = r.cl_status_col.clone().unwrap_or_default();
                self.active_tool = ActiveToolModal::ReconcilePeriods;
            }
            ToolRun::Join(r) => {
                self.active_tab = idx[0];
                let s = &mut self.join;
                s.left_tab = idx[0];
                s.right_tab = idx[1];
                s.kind = r.kind;
                s.keys_input = r.keys.join(", ");
                s.keep_first = r.keep_first_duplicate_right;
                self.active_tool = ActiveToolModal::JoinTabs;
            }
            ToolRun::Stack(r) => {
                // each file is read with its own detected schema when the dialog is used again
                let s = &mut self.stack;
                s.files_list = r.files.iter().map(|f| f.path.clone()).collect();
                s.include_source = r.include_source_col;
                s.schema_mode = 0;
                s.schema_path = None;
                self.active_tool = ActiveToolModal::StackFiles;
            }
            ToolRun::PivotTab(r) => {
                self.active_tab = idx[0];
                self.init_tool_defaults_from_active_tab();
                let step = r.pivot.clone().ok_or_else(|| "The saved pivot has no pivot step.".to_string())?;
                if let Some(spec) = step.as_pivot_spec() {
                    self.open_pivot_builder(Some(spec), false);
                    self.active_tool = ActiveToolModal::PivotBuilder;
                } else {
                    let cols = self.tabs.get(idx[0]).map(|t| t.available_columns()).unwrap_or_default();
                    let tool = self.prefill_from_step(&step, &cols).ok_or_else(|| "The pivot dialog could not be filled.".to_string())?;
                    self.active_tool = tool;
                }
            }
        }
        self.status_message = format!("{}: the dialog has the inputs of the saved run. Change what you need and run it.", run.tool_name());
        Ok(())
    }

    /// The "Save run" dialog: the name, a line saying what the run holds, Save and Cancel.
    pub(crate) fn render_save_run_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let Some(mut draft) = self.runs.draft.take() else {
            return ModalOutcome { close: true, step: None };
        };
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = RunsRects::default();
        let mut close = false;
        let mut save = false;
        let dir = runs_dir();
        let title = format!("{}  Save run", theme::ph::CLOCK_COUNTER_CLOCKWISE);
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(describe(&draft.run)).color(tk.ink)).wrap());
                ui.add(egui::Label::new(egui::RichText::new("Source tabs are saved by name, not by position. Rule, curve and map files are read again when the run is made.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                ui.add_space(theme::SPACE_S);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let r = ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(theme::FIELD_W_SORT_LIST));
                    rects.fields.push(("Name".to_string(), r.rect));
                    if r.changed() {
                        draft.confirm_replace = false;
                        draft.error.clear();
                    }
                });
                ui.label(egui::RichText::new(format!("Saved as {}", dir.join(format!("{}.json", draft.name.trim())).display())).size(theme::TEXT_LABEL).color(tk.muted));
                if !draft.error.is_empty() {
                    let r = ui.add(egui::Label::new(egui::RichText::new(&draft.error).color(tk.danger)).wrap());
                    rects.message = Some(r.rect);
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let label = if draft.confirm_replace { "Replace" } else { "Save" };
                    let ok = run_file_stem(&draft.name).is_ok();
                    let b = ui.add_enabled_ui(ok, |ui| theme::primary_button(ui, label)).inner;
                    rects.buttons.push((label.to_string(), b.rect));
                    if b.clicked() {
                        save = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        if save {
            match run_path(&dir, &draft.name) {
                Err(e) => draft.error = format!("{:#}", e),
                Ok(path) if path.exists() && !draft.confirm_replace => {
                    draft.error = format!("A saved run called '{}' exists. Choose Replace to overwrite it, or type another name.", draft.name.trim());
                    draft.confirm_replace = true;
                }
                Ok(_) => match save_run(&dir, &draft.name, &draft.run) {
                    Ok(p) => {
                        self.status_message = format!("Saved the run as {}.", p.display());
                        close = true;
                    }
                    Err(e) => draft.error = format!("{:#}", e),
                },
            }
        }
        self.runs.rects = rects;
        if !close {
            self.runs.draft = Some(draft);
        }
        ModalOutcome { close, step: None }
    }

    /// File ▸ "Saved tool runs": the runs of `portable_data/runs`, each with a "Re-run with changes" button.
    pub(crate) fn render_saved_runs_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = RunsRects::default();
        let mut close = false;
        let mut chosen: Option<ToolRun> = None;
        let saved = self.runs.saved.clone();
        let title = format!("{}  Saved tool runs", theme::ph::CLOCK_COUNTER_CLOCKWISE);
        theme::modal(&title).default_width(theme::CURVES_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                if saved.is_empty() {
                    ui.label(egui::RichText::new("No saved run yet. Open the menu of a result tab (right-click its name) and choose \"Save run\u{2026}\".").color(tk.muted));
                }
                egui::ScrollArea::vertical().id_salt("saved_runs").max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                    for (name, r) in &saved {
                        let row = ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Ok(s) = r {
                                    let b = ui.button("Re-run with changes");
                                    rects.buttons.push((format!("Re-run {}", name), b.rect));
                                    if b.clicked() {
                                        chosen = Some(s.run.clone());
                                    }
                                }
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| match r {
                                    Ok(s) => {
                                        ui.add(egui::Label::new(egui::RichText::new(format!("{}  \u{b7}  {}", name, describe(&s.run))).color(tk.ink)).truncate());
                                    }
                                    Err(e) => {
                                        ui.add(egui::Label::new(egui::RichText::new(format!("{}: {}", name, e)).color(tk.danger)).truncate());
                                    }
                                });
                            });
                        });
                        rects.rows.push((name.clone(), row.response.rect));
                    }
                });
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.runs.rects = rects;
        if let Some(run) = chosen {
            // on success the tool's dialog is now the active one; on failure this list stays open
            if let Err(e) = self.prefill_run(&run) {
                self.status_message = format!("Re-run: {}", e);
            }
            return ModalOutcome { close: false, step: None };
        }
        ModalOutcome { close, step: None }
    }
}
