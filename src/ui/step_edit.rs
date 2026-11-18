//! Editing a step in place (Phase 3 fix 10, owner answer to Q6): every step that a tool modal
//! makes reopens in that modal, prefilled; saving replaces the step and the later steps are
//! checked again.
//!
//! Each modal builds its step with [`MicroParquetApp::tool_step`], and [`prefill_from_step`]
//! is its inverse, so a step reopened and saved unchanged is the same step.
//!
//! [`prefill_from_step`]: MicroParquetApp::prefill_from_step

use crate::ui::prelude::*;

/// The modal that makes (and edits) a step. `None`: filters and sorts (their own editors),
/// Set to Number and SQL steps (no modal), and turned-off steps (turn them on to edit).
pub fn step_modal(step: &TransformStep) -> Option<ActiveToolModal> {
    Some(match step {
        TransformStep::SelectAndReorderColumns { .. } | TransformStep::RenameColumn { .. } => ActiveToolModal::ManageColumns,
        TransformStep::ConditionalColumn { .. } => ActiveToolModal::ConditionalCol,
        TransformStep::MathFormula { .. } => ActiveToolModal::MathFormula,
        TransformStep::NormalizeEntity { .. } => ActiveToolModal::NormalizeEntity,
        TransformStep::CleanLineBreaks { .. } => ActiveToolModal::CleanLineBreaks,
        TransformStep::MaturityBucket { .. } => ActiveToolModal::MaturityBucket,
        TransformStep::RateBand { .. } => ActiveToolModal::RateBand,
        TransformStep::RateShockSim { .. } => ActiveToolModal::RateShockSim,
        TransformStep::FtpCurveMap { .. } => ActiveToolModal::FtpCurveMap,
        TransformStep::BankingHygieneAudit { .. } => ActiveToolModal::BankingAudit,
        TransformStep::DurationAnalysis { .. } => ActiveToolModal::DurationAnalysis,
        TransformStep::LoanClassificationProvision { .. } => ActiveToolModal::LoanProvision,
        TransformStep::LoanLimitUtilization { .. } => ActiveToolModal::LoanLimitUtil,
        TransformStep::ClassifyOverdue { .. } => ActiveToolModal::ClassifyOverdue,
        TransformStep::GroupByPivot { .. } => ActiveToolModal::GroupByPivot,
        TransformStep::PivotTable { .. } => ActiveToolModal::PivotBuilder,
        TransformStep::FilterRows { .. }
        | TransformStep::FilterGroup { .. }
        | TransformStep::SortRows { .. }
        | TransformStep::ConvertToNumber { .. }
        | TransformStep::SqlQuery { .. }
        | TransformStep::Skipped { .. } => return None,
    })
}

/// True when step `step` can be edited in place (a tool modal, the filter or the sort editor).
pub fn step_is_editable(step: &TransformStep) -> bool {
    step_modal(step).is_some() || step.as_filter_group().is_some() || step.sort_levels().is_some()
}

/// The step being edited: which tab (by id), which step, and the columns that step sees.
#[derive(Clone, Debug)]
pub struct EditingStep {
    pub tab_id: u64,
    pub index: usize,
    pub columns: Vec<String>,
    /// The modal it was opened in (another modal opened meanwhile ends the edit).
    pub tool: ActiveToolModal,
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// The edited step, with what its modal cannot show taken from the original: a pivot's pivot
/// column, and its sort column while that is still one of the output columns (the modal would
/// sort by the first measure, which changes which rows a Top-N keeps).
pub fn keep_uneditable(original: &TransformStep, edited: TransformStep) -> TransformStep {
    match (original, edited) {
        (
            TransformStep::GroupByPivot { pivot_col: orig_pivot, sort_by_col: orig_sort, .. },
            TransformStep::GroupByPivot { group_cols, pivot_col, aggs, sort_by_col, top_n },
        ) => {
            let outputs: Vec<&String> = group_cols.iter().chain(aggs.iter().map(|a| &a.alias)).collect();
            let sort_by_col = match orig_sort {
                Some(s) if outputs.contains(&s) => Some(s.clone()),
                _ => sort_by_col,
            };
            TransformStep::GroupByPivot { group_cols, pivot_col: pivot_col.or_else(|| orig_pivot.clone()), aggs, sort_by_col, top_n }
        }
        (_, e) => e,
    }
}

impl MicroParquetApp {
    /// The step a tool modal makes from its current settings (`None` for modals that make no
    /// step, or a bucket modal without a chosen preset). The modals' buttons use this.
    pub(crate) fn tool_step(&self, tool: &ActiveToolModal) -> Option<TransformStep> {
        // the side column is kept as typed (not trimmed), as the modals always did
        let side = || if self.shock.side_col.trim().is_empty() { None } else { Some(self.shock.side_col.clone()) };
        Some(match tool {
            ActiveToolModal::ManageColumns => TransformStep::SelectAndReorderColumns {
                cols: self.columns.list.iter().filter(|(_, k, _)| *k).map(|(n, _, _)| n.clone()).collect(),
            },
            ActiveToolModal::ConditionalCol => TransformStep::ConditionalColumn {
                new_col: self.cond.new_col.clone(),
                rules: self.cond.rules.clone(),
                fallback: self.cond.fallback.clone(),
                output_as_float: self.cond.as_float,
                blank_result: self.cond.blank_rule(),
            },
            ActiveToolModal::MathFormula => TransformStep::MathFormula {
                new_col: self.calc.new_col.clone(),
                left_col: self.calc.left_col.clone(),
                op: self.calc.op,
                right_is_col: self.calc.right_is_col,
                right_val_or_col: self.calc.right_val.clone(),
            },
            ActiveToolModal::NormalizeEntity => TransformStep::NormalizeEntity {
                source_col: self.normaliser.source_col.clone(),
                new_col: self.normaliser.new_col.clone(),
            },
            ActiveToolModal::CleanLineBreaks => return self.clean_breaks_step(),
            ActiveToolModal::RateBand => {
                let band_def = self.rate_band.def().ok()?;
                if self.rate_band.new_col.trim().is_empty() {
                    return None;
                }
                TransformStep::RateBand { rate_col: self.rate_band.rate_col.clone(), band_def, new_col: self.rate_band.new_col.trim().to_string() }
            }
            ActiveToolModal::MaturityBucket => TransformStep::MaturityBucket {
                mat_col: self.buckets.mat_col.clone(),
                cutoff_date: self.buckets.cutoff.clone(),
                bucket_def: self.buckets.presets.get(self.buckets.selected_preset_idx).cloned()?,
                null_as_sight: self.buckets.null_sight,
                new_col: self.buckets.new_col.clone(),
            },
            ActiveToolModal::RateShockSim => TransformStep::RateShockSim {
                bal_col: self.shock.bal_col.clone(),
                rate_col: self.shock.rate_col.clone(),
                mat_col: self.shock.mat_col.clone(),
                cutoff_date: self.buckets.cutoff.clone(),
                shock_bps: self.shock.bps,
                side_col: side(),
                reprice_col: self.shock.reprice_choice(),
                beta_file: self.shock.beta_choice(),
                product_col: self.shock.product_choice(),
                floor_pct: self.shock.floor_choice(),
            },
            ActiveToolModal::DurationAnalysis => TransformStep::DurationAnalysis {
                bal_col: self.duration.bal_col.clone(),
                rate_col: self.duration.rate_col.clone(),
                mat_col: self.duration.mat_col.clone(),
                cutoff_date: self.buckets.cutoff.clone(),
                compounding_freq: self.duration.freq,
                shock_bps: self.duration.shock_bps,
                default_sight_years: self.duration.sight_yrs,
                side_col: side(),
            },
            ActiveToolModal::FtpCurveMap => TransformStep::FtpCurveMap {
                opndat_col: self.ftp.opn_col.clone(),
                matdat_col: self.ftp.mat_col.clone(),
                bal_col: self.ftp.bal_col.clone(),
                rate_col: self.ftp.rate_col.clone(),
                // the typed points belong to the step only when it uses them; a step that reads the store has none
                curve: if self.ftp.curve_source == crate::ui::ftp::CurveSource::Typed { self.ftp.curve.clone() } else { Vec::new() },
                is_liability: self.ftp.is_liability,
                interpolation: self.ftp.interpolation,
                curve_mode: self.ftp.curve_mode(),
                currency: self.ftp.currency.trim().to_string(),
            },
            ActiveToolModal::BankingAudit => TransformStep::BankingHygieneAudit {
                opndat_col: self.audit.opn_col.clone(),
                matdat_col: self.audit.mat_col.clone(),
                clsdat_col: self.audit.cls_col.clone(),
                acstat_col: self.audit.stat_col.clone(),
                bal_col: self.audit.bal_col.clone(),
                rate_col: self.audit.rate_col.clone(),
                actnum_col: self.audit.key_col.clone(),
                cutoff_date: self.buckets.cutoff.clone(),
                only_exceptions: self.audit.only_exceptions,
                active_statuses: self
                    .audit
                    .active_statuses
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                rate_min_pct: self.audit.rate_min,
                rate_max_pct: self.audit.rate_max,
            },
            ActiveToolModal::LoanProvision => TransformStep::LoanClassificationProvision {
                bal_col: self.provision.bal_col.clone(),
                cl_status_col: self.provision.cl_col.clone(),
                segment_col: opt(&self.provision.seg_col),
                suspense_col: opt(&self.provision.susp_col),
                security_col: opt(&self.provision.sec_col),
                cutoff_date: self.provision.cutoff.trim().to_string(),
                collateral_type_col: opt(&self.provision.coll_type_col),
            },
            ActiveToolModal::ClassifyOverdue => return self.classify_overdue_step().ok(),
            ActiveToolModal::LoanLimitUtil => TransformStep::LoanLimitUtilization {
                limit_col: self.limits.col.clone(),
                bal_col: self.limits.bal_col.clone(),
                ccf_pct: self.limits.ccf_pct,
            },
            ActiveToolModal::GroupByPivot => TransformStep::GroupByPivot {
                group_cols: self.pivot.group_cols.iter().filter(|(_, c)| *c).map(|(n, _)| n.clone()).collect(),
                pivot_col: None,
                aggs: self.pivot.aggs.clone(),
                sort_by_col: self.pivot.aggs.first().map(|a| a.alias.clone()),
                top_n: if self.pivot.top_n > 0 { Some(self.pivot.top_n) } else { None },
            },
            ActiveToolModal::PivotBuilder => return self.pivot_builder_step(),
            _ => return None,
        })
    }

    /// The Columns modal's rename (its second button).
    pub(crate) fn rename_step(&self) -> Option<TransformStep> {
        let new_name = self.columns.rename_new.trim();
        if new_name.is_empty() {
            return None;
        }
        Some(TransformStep::RenameColumn { old_name: self.columns.rename_old.clone(), new_name: new_name.to_string() })
    }

    /// Puts a step's settings into its modal (the inverse of `tool_step`). `columns` are the
    /// columns the step sees. Returns the modal, or `None` when the step has none.
    pub(crate) fn prefill_from_step(&mut self, step: &TransformStep, columns: &[String]) -> Option<ActiveToolModal> {
        let tool = step_modal(step)?;
        match step.clone() {
            TransformStep::SelectAndReorderColumns { cols } => {
                let fmt = |c: &str| {
                    self.tabs
                        .get(self.active_tab)
                        .and_then(|t| t.col_formats.get(c).copied())
                        .unwrap_or(ColumnFormatSpec { fmt_type: MetricFormatType::Auto, decimals: self.global_decimals })
                };
                let mut list: Vec<(String, bool, ColumnFormatSpec)> = cols.iter().map(|c| (c.clone(), true, fmt(c))).collect();
                list.extend(columns.iter().filter(|c| !cols.contains(c)).map(|c| (c.clone(), false, fmt(c))));
                self.columns.list = list;
            }
            TransformStep::RenameColumn { old_name, new_name } => {
                self.columns.rename_old = old_name;
                self.columns.rename_new = new_name;
            }
            TransformStep::ConditionalColumn { new_col, rules, fallback, output_as_float, blank_result } => {
                self.cond.new_col = new_col;
                self.cond.rules = rules;
                self.cond.fallback = fallback;
                self.cond.as_float = output_as_float;
                self.cond.set_blank_rule(&blank_result);
            }
            TransformStep::MathFormula { new_col, left_col, op, right_is_col, right_val_or_col } => {
                self.calc.new_col = new_col;
                self.calc.left_col = left_col;
                self.calc.op = op;
                self.calc.right_is_col = right_is_col;
                self.calc.right_val = right_val_or_col;
            }
            TransformStep::CleanLineBreaks { columns, trim_all } => {
                self.clean_breaks.all = columns.is_empty();
                self.clean_breaks.trim_all = trim_all;
                self.clean_breaks.picked = columns;
            }
            TransformStep::NormalizeEntity { source_col, new_col } => {
                self.normaliser.source_col = source_col;
                self.normaliser.new_col = new_col;
            }
            TransformStep::RateBand { rate_col, band_def, new_col } => {
                self.rate_band.load(&band_def);
                self.rate_band.rate_col = rate_col;
                self.rate_band.new_col = new_col;
            }
            TransformStep::MaturityBucket { mat_col, cutoff_date, bucket_def, null_as_sight, new_col } => {
                // The step's own bucket definition; added to the list (not saved) when it is
                // not one of the presets.
                let same = |p: &CustomBucketDef| format!("{:?}", p) == format!("{:?}", bucket_def);
                let idx = match self.buckets.presets.iter().position(same) {
                    Some(i) => i,
                    None => {
                        self.buckets.presets.push(bucket_def);
                        self.buckets.presets.len() - 1
                    }
                };
                self.buckets.selected_preset_idx = idx;
                self.buckets.mat_col = mat_col;
                self.buckets.cutoff = cutoff_date;
                self.buckets.null_sight = null_as_sight;
                self.buckets.new_col = new_col;
            }
            TransformStep::RateShockSim { bal_col, rate_col, mat_col, cutoff_date, shock_bps, side_col, reprice_col, beta_file, product_col, floor_pct } => {
                self.shock.reprice_col = reprice_col.unwrap_or_default();
                self.shock.beta_file = beta_file.unwrap_or_default();
                self.shock.product_col = product_col.unwrap_or_default();
                self.shock.use_floor = floor_pct.is_some();
                self.shock.floor_pct = floor_pct.unwrap_or(0.0);
                self.shock.reload_beta_files();
                self.shock.bal_col = bal_col;
                self.shock.rate_col = rate_col;
                self.shock.mat_col = mat_col;
                self.buckets.cutoff = cutoff_date;
                self.shock.bps = shock_bps;
                self.shock.side_col = side_col.unwrap_or_default();
                // the side picker keeps this choice (it defaults only when the columns change)
                self.shock.side_cols_seen = columns.to_vec();
            }
            TransformStep::DurationAnalysis {
                bal_col,
                rate_col,
                mat_col,
                cutoff_date,
                compounding_freq,
                shock_bps,
                default_sight_years,
                side_col,
            } => {
                self.duration.bal_col = bal_col;
                self.duration.rate_col = rate_col;
                self.duration.mat_col = mat_col;
                self.buckets.cutoff = cutoff_date;
                self.duration.freq = compounding_freq;
                self.duration.shock_bps = shock_bps;
                self.duration.sight_yrs = default_sight_years;
                self.shock.side_col = side_col.unwrap_or_default();
                self.shock.side_cols_seen = columns.to_vec();
            }
            TransformStep::FtpCurveMap { opndat_col, matdat_col, bal_col, rate_col, curve, is_liability, interpolation, curve_mode, currency } => {
                self.ftp.currency = currency;
                self.ftp.opn_col = opndat_col;
                self.ftp.mat_col = matdat_col;
                self.ftp.bal_col = bal_col;
                self.ftp.rate_col = rate_col;
                if !curve.is_empty() {
                    self.ftp.curve = curve;
                }
                self.ftp.is_liability = is_liability;
                self.ftp.interpolation = interpolation;
                self.ftp.set_curve_mode(&curve_mode);
                self.ftp.reload_store();
            }
            TransformStep::BankingHygieneAudit {
                opndat_col,
                matdat_col,
                clsdat_col,
                acstat_col,
                bal_col,
                rate_col,
                actnum_col,
                cutoff_date,
                only_exceptions,
                active_statuses,
                rate_min_pct,
                rate_max_pct,
            } => {
                self.audit.opn_col = opndat_col;
                self.audit.mat_col = matdat_col;
                self.audit.cls_col = clsdat_col;
                self.audit.stat_col = acstat_col;
                self.audit.bal_col = bal_col;
                self.audit.rate_col = rate_col;
                self.audit.key_col = actnum_col;
                self.buckets.cutoff = cutoff_date;
                self.audit.only_exceptions = only_exceptions;
                self.audit.active_statuses = active_statuses.join(", ");
                self.audit.rate_min = rate_min_pct;
                self.audit.rate_max = rate_max_pct;
            }
            TransformStep::LoanClassificationProvision {
                bal_col,
                cl_status_col,
                segment_col,
                suspense_col,
                security_col,
                cutoff_date,
                collateral_type_col,
            } => {
                self.provision.bal_col = bal_col;
                self.provision.cl_col = cl_status_col;
                self.provision.seg_col = segment_col.unwrap_or_default();
                self.provision.susp_col = suspense_col.unwrap_or_default();
                self.provision.sec_col = security_col.unwrap_or_default();
                self.provision.cutoff = cutoff_date;
                self.provision.coll_type_col = collateral_type_col.unwrap_or_default();
            }
            TransformStep::ClassifyOverdue { category_col, fixed_category, since_col, dpd_col, override_col, cutoff_date } => {
                let o = &mut self.overdue;
                o.category_by_col = category_col.as_deref().is_some_and(|c| !c.trim().is_empty());
                o.category_col = category_col.unwrap_or_default();
                if !fixed_category.trim().is_empty() {
                    o.fixed_category = fixed_category;
                }
                o.use_dpd = dpd_col.as_deref().is_some_and(|c| !c.trim().is_empty());
                o.since_col = since_col.unwrap_or_default();
                o.dpd_col = dpd_col.unwrap_or_default();
                o.override_on = override_col.as_deref().is_some_and(|c| !c.trim().is_empty());
                o.override_col = override_col.unwrap_or_default();
                o.cutoff = cutoff_date;
                o.categories = None;
            }
            TransformStep::LoanLimitUtilization { limit_col, bal_col, ccf_pct } => {
                self.limits.col = limit_col;
                self.limits.bal_col = bal_col;
                self.limits.ccf_pct = ccf_pct;
            }
            TransformStep::GroupByPivot { group_cols, aggs, top_n, .. } => {
                let mut list: Vec<(String, bool)> = columns.iter().map(|c| (c.clone(), group_cols.contains(c))).collect();
                // a grouping column the step names but the source lacks stays visible (and ticked)
                list.extend(group_cols.iter().filter(|g| !columns.contains(g)).map(|g| (g.clone(), true)));
                self.pivot.group_cols = list;
                self.pivot.aggs = aggs;
                self.pivot.top_n = top_n.unwrap_or(0);
                self.pivot.open_in_new_tab = false;
            }
            TransformStep::PivotTable { .. } => {
                if let Some(spec) = step.as_pivot_spec() {
                    self.open_pivot_builder(Some(spec), true);
                }
            }
            _ => return None,
        }
        Some(tool)
    }

    /// The columns step `i` of the active tab sees (the result of the steps before it).
    fn columns_before_step(&self, i: usize) -> anyhow::Result<Vec<String>> {
        let tab = self.tabs.get(self.active_tab).ok_or_else(|| anyhow::anyhow!("No tab is open."))?;
        let mut lf = compile_tab_lazy(&tab.source, &tab.steps[..i.min(tab.steps.len())], &self.paths.temp_dir)?;
        let schema = lf.collect_schema()?;
        Ok(schema.iter_names().map(|n| n.to_string()).collect())
    }

    /// Opens the editor of step `i` of the active tab, prefilled. False when it has none.
    pub(crate) fn open_step_editor(&mut self, i: usize) -> bool {
        let Some(step) = self.tabs.get(self.active_tab).and_then(|t| t.steps.get(i)).cloned() else {
            return false;
        };
        if step.as_filter_group().is_some() {
            self.open_filter_editor(Some(i), false);
            return true;
        }
        if step.sort_levels().is_some() {
            self.open_sort_editor_at(Some(i), None);
            return true;
        }
        if step_modal(&step).is_none() {
            return false;
        }
        self.init_tool_defaults_from_active_tab();
        let columns = match self.columns_before_step(i) {
            Ok(c) => c,
            Err(e) => {
                self.status_message = format!(
                    "Could not read the columns step {} sees ({:#}); the editor lists the table's columns.",
                    i + 1,
                    e
                );
                self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default()
            }
        };
        let Some(tool) = self.prefill_from_step(&step, &columns) else {
            return false;
        };
        let tab_id = self.tabs.get(self.active_tab).map(|t| t.id).unwrap_or_default();
        self.editing = Some(EditingStep { tab_id, index: i, columns, tool });
        self.active_tool = tool;
        true
    }

    /// A modal produced `step` while a step is being edited: it replaces that step (what the
    /// modal cannot show is kept), and the steps after it are checked again by the refresh.
    pub(crate) fn save_edited_step(&mut self, step: TransformStep) {
        let Some(ed) = self.editing.clone() else {
            return;
        };
        let Some(pos) = self.tabs.iter().position(|t| t.id == ed.tab_id) else {
            self.status_message = "The tab of the edited step was closed; nothing was changed.".to_string();
            return;
        };
        self.active_tab = pos;
        let Some(original) = self.tabs.get(pos).and_then(|t| t.steps.get(ed.index)).cloned() else {
            self.status_message = "The edited step is no longer in the list; nothing was changed.".to_string();
            return;
        };
        let step = keep_uneditable(&original, step);
        let status = format!("Step {} edited: {}.", ed.index + 1, step.short_label());
        self.replace_step_in_active_tab(ed.index, step, &status);
        self.selected_step = Some(ed.index);
    }

    /// The line above the modal while a step is edited.
    pub(crate) fn render_edit_banner(&self, ctx: &egui::Context) {
        let Some(ed) = &self.editing else {
            return;
        };
        let tk = *theme::tokens(self.dark_mode);
        let label = self
            .tabs
            .iter()
            .find(|t| t.id == ed.tab_id)
            .and_then(|t| t.steps.get(ed.index))
            .map(|s| s.short_label())
            .unwrap_or_default();
        egui::Area::new(egui::Id::new("edit_step_banner"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, theme::TOP_BAR_H + theme::EDIT_BANNER_GAP))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(tk.accent_soft)
                    .stroke(theme::line(tk.accent))
                    .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
                    .inner_margin(theme::MARGIN_EDIT_BANNER)
                    .show(ui, |ui| {
                        ui.label(theme::semibold(format!(
                            "Editing step {} '{}': the modal's main button saves it in place; the steps after it are checked again.",
                            ed.index + 1,
                            label
                        )));
                    });
            });
    }
}
