//! One module per screen (Phase 3 R1). `theme` holds every colour and size token.

pub mod audit;
pub mod batch;
pub mod buckets;
pub mod calc_column;
pub mod chart;
pub mod columns;
pub mod commands;
pub mod conditional;
pub mod duration;
pub mod export;
pub mod filter;
pub mod ftp;
pub mod jobs;
pub mod join;
pub mod limits;
pub mod normaliser;
pub mod pivot;
pub mod prelude;
pub mod provision;
pub mod rate_shock;
pub mod reconcile;
pub mod schema;
pub mod sort;
pub mod sql;
pub mod stack;
pub mod status;
pub mod step_edit;
pub mod steps;
pub mod table;
pub mod tabs;
pub mod top_bar;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;
pub mod theme;

use crate::ui::prelude::*;

/// What a tool modal asks the app to do after it was drawn this frame.
pub struct ModalOutcome {
    /// Close the modal.
    pub close: bool,
    /// A step to add to the active tab (second field: into a new tab).
    pub step: Option<(TransformStep, bool)>,
}

impl MicroParquetApp {
    pub(crate) fn render_active_tool_modal(&mut self, ctx: &egui::Context) {
        if self.active_tool == ActiveToolModal::None {
            return;
        }

        // An edit belongs to the modal it was opened in.
        if self.editing.as_ref().is_some_and(|e| e.tool != self.active_tool) {
            self.editing = None;
        }
        // While a step is edited, its modal lists the columns that step sees.
        let avail_cols = match &self.editing {
            Some(ed) => ed.columns.clone(),
            None => self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default(),
        };

        // Default the side picker whenever the column set changes (not every frame, so a
        // deliberate "none" choice sticks): first column named like side/direction.
        if matches!(
            self.active_tool,
            ActiveToolModal::RateShockSim | ActiveToolModal::DurationAnalysis
        ) && self.shock.side_cols_seen != avail_cols
        {
            self.shock.side_cols_seen = avail_cols.clone();
            self.shock.side_col = rate_shock::default_side_column(&avail_cols);
        }

        let cols = avail_cols;
        let outcome = match self.active_tool {
            ActiveToolModal::FilterRows => self.render_filter_modal(ctx, cols),
            ActiveToolModal::SortRows => self.render_sort_modal(ctx, cols),
            ActiveToolModal::ManageColumns => self.render_columns_modal(ctx, cols),
            ActiveToolModal::ConditionalCol => self.render_conditional_modal(ctx, cols),
            ActiveToolModal::MathFormula => self.render_calc_modal(ctx, cols),
            ActiveToolModal::NormalizeEntity => self.render_normaliser_modal(ctx, cols),
            ActiveToolModal::GroupByPivot => self.render_pivot_modal(ctx, cols),
            ActiveToolModal::MaturityBucket => self.render_buckets_modal(ctx, cols),
            ActiveToolModal::RateShockSim => self.render_rate_shock_modal(ctx, cols),
            ActiveToolModal::DurationAnalysis => self.render_duration_modal(ctx, cols),
            ActiveToolModal::LoanProvision => self.render_provision_modal(ctx, cols),
            ActiveToolModal::LoanLimitUtil => self.render_limits_modal(ctx, cols),
            ActiveToolModal::FtpCurveMap => self.render_ftp_modal(ctx, cols),
            ActiveToolModal::BankingAudit => self.render_audit_modal(ctx, cols),
            ActiveToolModal::StackFiles => self.render_stack_modal(ctx, cols),
            ActiveToolModal::JoinTabs => self.render_join_modal(ctx, cols),
            ActiveToolModal::ReconcilePeriods => self.render_reconcile_modal(ctx, cols),
            ActiveToolModal::SaveMacro => self.render_save_macro_modal(ctx, cols),
            ActiveToolModal::PythonCodePreview => self.render_python_preview_modal(ctx, cols),
            ActiveToolModal::MultiTabExport => self.render_multi_export_modal(ctx, cols),
            ActiveToolModal::SplitByColumn => self.render_split_modal(ctx, cols),
            ActiveToolModal::None => ModalOutcome { close: false, step: None },
        };

        if let Some((step, new_tab)) = outcome.step {
            if self.editing.is_some() {
                // Edit in place: the step is replaced in its own tab (never a new tab).
                self.save_edited_step(step);
            } else {
                self.push_step_to_active_tab(step, new_tab);
            }
        }
        self.render_edit_banner(ctx);
        if outcome.close {
            self.active_tool = ActiveToolModal::None;
            self.editing = None;
            // "Insert after" applies to the step this modal produced, never to a later one.
            self.insert_after = None;
        }
    }
}
