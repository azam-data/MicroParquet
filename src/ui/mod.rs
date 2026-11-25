//! One module per screen (Phase 3 R1). `theme` holds every colour and size token.

pub mod audit;
pub mod auto_order;
pub mod batch;
pub mod buckets;
pub mod calc_column;
pub mod calendar;
pub mod chart_filters;
pub mod colour_picker;
pub mod chart;
pub mod chart_render;
pub mod chart_studio;
pub mod chart_tab;
pub mod drill;
pub mod cl_migration;
pub mod columns;
pub mod commands;
pub mod conditional;
pub mod curves;
pub mod dashboard;
pub mod dq;
pub mod duration;
pub mod export;
pub mod save_dialog;
pub mod filter;
pub mod ftp;
pub mod behaviour_profile;
pub mod cashflow_ladder;
pub mod gap_ladder;
pub mod irrbb;
pub mod jobs;
pub mod join;
pub mod limits;
pub mod model;
pub mod model_calendar;
pub mod model_measures;
pub mod mom;
pub mod mudaraba;
pub mod clean_breaks;
pub mod classify_overdue;
pub mod cl1_summary;
pub mod ifrs9;
pub mod liquidity;
pub mod pick;
pub mod normaliser;
pub mod pivot;
pub mod pivot_bar;
pub mod pivot_builder;
pub mod pivot_layout;
pub mod prelude;
pub mod project;
pub mod provision;
pub mod rate_band;
pub mod rate_shock;
pub mod packs;
pub mod runs;
pub mod reconcile;
pub mod schema;
pub mod sort;
pub mod sort_lists;
pub mod split_date;
pub mod sql;
pub mod stack;
pub mod status;
pub mod step_edit;
pub mod steps;
pub mod table;
pub mod tabs;
pub mod top_bar;
pub mod value_picker;
pub mod zoom;
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod render_tests_p4;
#[cfg(test)]
mod render_tests_p5;
#[cfg(test)]
mod render_tests_p6;
#[cfg(test)]
mod render_tests_p7;
#[cfg(test)]
mod render_tests_p8;
#[cfg(test)]
mod render_tests_p9;
#[cfg(test)]
mod render_tests_p10;
#[cfg(test)]
mod render_tests_r;
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

        // a dialog starts with a new value picker and the lists as they are on disk
        if self.picker_tool != self.active_tool {
            self.picker = PickerPanel::default();
            self.picker_tool = self.active_tool;
            self.sort_lists.reload();
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
            ActiveToolModal::SortLists => self.render_sort_lists_modal(ctx, cols),
            ActiveToolModal::Calendar => self.render_calendar_modal(ctx, cols),
            ActiveToolModal::ColourPicker => self.render_colour_picker_modal(ctx, cols),
            ActiveToolModal::HouseColours => self.render_house_colours_modal(ctx, cols),
            ActiveToolModal::ProjectSources => self.render_project_sources_modal(ctx, cols),
            ActiveToolModal::ProjectSave => self.render_project_save_modal(ctx, cols),
            ActiveToolModal::ProjectPrompt => self.render_project_prompt_modal(ctx, cols),
            ActiveToolModal::ProjectRecent => self.render_project_recent_modal(ctx, cols),
            ActiveToolModal::ModelLinks => self.render_model_links_modal(ctx, cols),
            ActiveToolModal::ModelKeyCheck => self.render_model_key_check_modal(ctx, cols),
            ActiveToolModal::ModelNoProject => self.render_model_no_project_modal(ctx, cols),
            ActiveToolModal::ModelCalendar => self.render_model_calendar_modal(ctx, cols),
            ActiveToolModal::ModelAsOf => self.render_model_as_of_modal(ctx, cols),
            ActiveToolModal::ModelMeasures => self.render_model_measures_modal(ctx, cols),
            ActiveToolModal::SplitDate => self.render_split_date_modal(ctx, cols),
            ActiveToolModal::ChartGallery => self.render_chart_gallery_modal(ctx, cols),
            ActiveToolModal::ChartSave => self.render_chart_save_modal(ctx, cols),
            ActiveToolModal::ChartOpen => self.render_chart_open_modal(ctx, cols),
            ActiveToolModal::ChartBook => self.render_chart_book_modal(ctx, cols),
            ActiveToolModal::Mudaraba => self.render_mudaraba_modal(ctx, cols),
            ActiveToolModal::DqScorecard => self.render_dq_modal(ctx, cols),
            ActiveToolModal::MomVariance => self.render_mom_modal(ctx, cols),
            ActiveToolModal::Packs => self.render_packs_modal(ctx, cols),
            ActiveToolModal::SaveRun => self.render_save_run_modal(ctx, cols),
            ActiveToolModal::SavedRuns => self.render_saved_runs_modal(ctx, cols),
            ActiveToolModal::ManageColumns => self.render_columns_modal(ctx, cols),
            ActiveToolModal::ConditionalCol => self.render_conditional_modal(ctx, cols),
            ActiveToolModal::MathFormula => self.render_calc_modal(ctx, cols),
            ActiveToolModal::NormalizeEntity => self.render_normaliser_modal(ctx, cols),
            ActiveToolModal::CleanLineBreaks => self.render_clean_breaks_modal(ctx, cols),
            ActiveToolModal::GroupByPivot => self.render_pivot_modal(ctx, cols),
            ActiveToolModal::PivotBuilder => self.render_pivot_builder(ctx, cols),
            ActiveToolModal::MaturityBucket => self.render_buckets_modal(ctx, cols),
            ActiveToolModal::RateBand => self.render_rate_band_modal(ctx, cols),
            ActiveToolModal::RateShockSim => self.render_rate_shock_modal(ctx, cols),
            ActiveToolModal::DurationAnalysis => self.render_duration_modal(ctx, cols),
            ActiveToolModal::LoanProvision => self.render_provision_modal(ctx, cols),
            ActiveToolModal::ClassifyOverdue => self.render_classify_overdue_modal(ctx, cols),
            ActiveToolModal::Cl1Summary => self.render_cl1_modal(ctx, cols),
            ActiveToolModal::Liquidity => self.render_liquidity_modal(ctx, cols),
            ActiveToolModal::IfrsEcl => self.render_ifrs9_modal(ctx, cols),
            ActiveToolModal::LoanLimitUtil => self.render_limits_modal(ctx, cols),
            ActiveToolModal::FtpCurveMap => self.render_ftp_modal(ctx, cols),
            ActiveToolModal::BankingAudit => self.render_audit_modal(ctx, cols),
            ActiveToolModal::StackFiles => self.render_stack_modal(ctx, cols),
            ActiveToolModal::JoinTabs => self.render_join_modal(ctx, cols),
            ActiveToolModal::ReconcilePeriods => self.render_reconcile_modal(ctx, cols),
            ActiveToolModal::ClMigration => self.render_cl_migration_modal(ctx, cols),
            ActiveToolModal::GapLadder => self.render_gap_ladder_modal(ctx, cols),
            ActiveToolModal::IrrbbLadder => self.render_cf_ladder_modal(ctx, cols),
            ActiveToolModal::IrrbbSix => self.render_irrbb_modal(ctx, cols),
            ActiveToolModal::BehaviourProfile => self.render_behaviour_modal(ctx, cols),
            ActiveToolModal::YieldCurves => self.render_curves_modal(ctx, cols),
            ActiveToolModal::SaveMacro => self.render_save_macro_modal(ctx, cols),
            ActiveToolModal::PythonCodePreview => self.render_python_preview_modal(ctx, cols),
            ActiveToolModal::MultiTabExport => self.render_multi_export_modal(ctx, cols),
            ActiveToolModal::SplitByColumn => self.render_split_modal(ctx, cols),
            ActiveToolModal::None => ModalOutcome { close: false, step: None },
        };

        self.apply_modal_outcome(Some(ctx), outcome);
    }

    /// What the app does with what a tool dialog asked for: add or replace the step, then close it. `ctx` is the window the
    /// edit banner is drawn in (none where nothing is drawn).
    pub(crate) fn apply_modal_outcome(&mut self, ctx: Option<&egui::Context>, outcome: ModalOutcome) {
        if let Some((step, new_tab)) = outcome.step {
            if self.editing.is_some() {
                // Edit in place: the step is replaced in its own tab (never a new tab).
                self.save_edited_step(step);
            } else {
                self.push_step_to_active_tab(step, new_tab);
            }
        }
        // the IFRS 9 summary opens after the step was added to the tab it reads
        if let Some(source) = self.pending_ecl_summary.take() {
            self.create_tab_from_source_with("IFRS 9 summary".to_string(), source, Vec::new(), HashMap::new(), None);
        }
        if let Some(ctx) = ctx {
            self.render_edit_banner(ctx);
        }
        if outcome.close {
            self.active_tool = ActiveToolModal::None;
            self.editing = None;
            // "Insert after" applies to the step this modal produced, never to a later one.
            self.insert_after = None;
        }
    }
}
