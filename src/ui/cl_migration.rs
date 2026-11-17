//! The CL migration matrix dialog (Phase 4 V2): pick the previous and the current month's tabs,
//! the account key, balance and CL status columns (and a segment), and build the matrix in a new tab.

use crate::ui::prelude::*;
use micro_parquet::banking::{ClMigrationSpec, MigrationMeasure, MIGRATION_ALL_SEGMENTS, MIGRATION_CLOSED_COL};

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct MigrationRects {
    pub grid: egui::Rect,
    pub combos: Vec<(String, egui::Rect)>,
    pub legend: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
}

impl Default for MigrationRects {
    fn default() -> Self {
        Self { grid: egui::Rect::NOTHING, combos: Vec::new(), legend: egui::Rect::NOTHING, buttons: Vec::new() }
    }
}

/// The settings of the dialog.
pub struct ClMigrationState {
    pub prev_tab: usize,
    pub curr_tab: usize,
    pub key_col: String,
    pub bal_col: String,
    pub cl_col: String,
    /// Empty: no segment.
    pub segment_col: String,
    pub measure: MigrationMeasure,
    pub pct_of_row: bool,
    pub rects: MigrationRects,
}

impl Default for ClMigrationState {
    fn default() -> Self {
        Self {
            prev_tab: 0,
            curr_tab: 0,
            key_col: String::new(),
            bal_col: String::new(),
            cl_col: String::new(),
            segment_col: String::new(),
            measure: MigrationMeasure::Balance,
            pct_of_row: false,
            rects: MigrationRects::default(),
        }
    }
}

/// The first column whose name contains one of `words` (compared in lower case), else `None`.
fn guess_column(columns: &[String], words: &[&str]) -> Option<String> {
    columns.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned()
}

impl MicroParquetApp {
    /// Opens the dialog with the open tab as the current month and the one before it as the previous.
    pub(crate) fn open_cl_migration(&mut self) {
        let n = self.tabs.len();
        let curr = self.active_tab.min(n.saturating_sub(1));
        let prev = if curr > 0 { curr - 1 } else { 1.min(n.saturating_sub(1)) };
        self.cl_migration.prev_tab = prev;
        self.cl_migration.curr_tab = curr;
        self.pick_migration_columns();
        self.active_tool = ActiveToolModal::ClMigration;
    }

    /// The columns both months have.
    fn migration_common_columns(&self) -> Vec<String> {
        let a = self.tabs.get(self.cl_migration.prev_tab).map(|t| t.available_columns()).unwrap_or_default();
        let b = self.tabs.get(self.cl_migration.curr_tab).map(|t| t.available_columns()).unwrap_or_default();
        a.into_iter().filter(|c| b.contains(c)).collect()
    }

    /// Chooses columns that exist in both months, keeping the user's choice when it still does.
    fn pick_migration_columns(&mut self) {
        let common = self.migration_common_columns();
        let keep = |cur: &str, guess: Option<String>| -> String {
            if !cur.is_empty() && common.iter().any(|c| c == cur) {
                cur.to_string()
            } else {
                guess.unwrap_or_default()
            }
        };
        let cl = &self.cl_migration;
        let key = keep(&cl.key_col, guess_column(&common, &["actnum", "account", "acct"]).or_else(|| common.first().cloned()));
        let bal = keep(&cl.bal_col, guess_column(&common, &["balance", "outstanding", "amount"]));
        let status = keep(&cl.cl_col, guess_column(&common, &["cl_status", "clstatus", "classification", "cl status", "status"]));
        let segment = if cl.segment_col.is_empty() || common.iter().any(|c| *c == cl.segment_col) { cl.segment_col.clone() } else { String::new() };
        self.cl_migration.key_col = key;
        self.cl_migration.bal_col = bal;
        self.cl_migration.cl_col = status;
        self.cl_migration.segment_col = segment;
    }

    /// The matrix tab the dialog would make, or why it cannot be made yet.
    pub(crate) fn cl_migration_source(&self) -> Result<(String, TabSource), String> {
        let s = &self.cl_migration;
        if self.tabs.len() < 2 {
            return Err("Open the previous month and the current month in two tabs first.".to_string());
        }
        let (Some(p), Some(c)) = (self.tabs.get(s.prev_tab), self.tabs.get(s.curr_tab)) else {
            return Err("The selected previous or current tab no longer exists. Pick both tabs again.".to_string());
        };
        if s.prev_tab == s.curr_tab {
            return Err("Choose two different tabs: the previous month and the current month.".to_string());
        }
        for (what, v) in [("account key", &s.key_col), ("balance", &s.bal_col), ("CL status", &s.cl_col)] {
            if v.trim().is_empty() {
                return Err(format!("Choose the {} column (both months need a column of that name).", what));
            }
        }
        let spec = ClMigrationSpec {
            key_col: s.key_col.clone(),
            bal_col: s.bal_col.clone(),
            cl_col: s.cl_col.clone(),
            segment_col: if s.segment_col.trim().is_empty() { None } else { Some(s.segment_col.clone()) },
            measure: s.measure,
            pct_of_row: s.pct_of_row,
        };
        let title = format!("CL migration ({} to {})", p.title, c.title);
        Ok((
            title,
            TabSource::ClMigration {
                prev_source: Box::new(p.source.clone()),
                prev_steps: p.steps.clone(),
                curr_source: Box::new(c.source.clone()),
                curr_steps: c.steps.clone(),
                spec,
            },
        ))
    }

    /// The formats of the matrix columns: money in the View scale, counts as whole numbers, shares as percentages.
    fn migration_formats(&self) -> HashMap<String, ColumnFormatSpec> {
        let fmt = if self.cl_migration.pct_of_row {
            Some(ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 })
        } else if self.cl_migration.measure == MigrationMeasure::Count {
            Some(ColumnFormatSpec { fmt_type: MetricFormatType::StandardCommas, decimals: 0 })
        } else {
            None
        };
        let mut out = HashMap::new();
        if let Some(f) = fmt {
            for g in micro_parquet::banking::cl_grades().iter().chain([MIGRATION_CLOSED_COL, "Total"].iter()) {
                out.insert(g.to_string(), f);
            }
        }
        out
    }

    pub(crate) fn render_cl_migration_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  CL migration matrix", theme::ph::ARROWS_LEFT_RIGHT);
        let mut rects = MigrationRects::default();
        let mut build = false;
        // columns may have changed (another tab chosen)
        self.pick_migration_columns();
        let problem = self.cl_migration_source().err();
        theme::modal(&title).default_width(theme::MIGRATION_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                if self.tabs.len() < 2 {
                    ui.label("Open the previous month and the current month in two tabs first.");
                    return;
                }
                let titles: Vec<(usize, String)> = self.tabs.iter().enumerate().map(|(i, t)| (i, t.title.clone())).collect();
                let common = self.migration_common_columns();
                let grid = egui::Grid::new("cl_migration_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    let mut combo = |ui: &mut egui::Ui, label: &str, id: &str, current: String, options: Vec<(usize, String)>, pick: &mut dyn FnMut(usize)| {
                        ui.label(label);
                        let r = egui::ComboBox::from_id_salt(id).width(theme::MIGRATION_COMBO_W).selected_text(current).show_ui(ui, |ui| {
                            for (i, text) in &options {
                                if ui.selectable_label(false, text).clicked() {
                                    pick(*i);
                                }
                            }
                        });
                        rects.combos.push((label.to_string(), r.response.rect));
                        ui.end_row();
                    };
                    let prev_title = titles.get(self.cl_migration.prev_tab).map(|t| t.1.clone()).unwrap_or_default();
                    let curr_title = titles.get(self.cl_migration.curr_tab).map(|t| t.1.clone()).unwrap_or_default();
                    let mut new_prev = self.cl_migration.prev_tab;
                    let mut new_curr = self.cl_migration.curr_tab;
                    combo(ui, "Previous-month tab:", "clm_prev", prev_title, titles.clone(), &mut |i| new_prev = i);
                    combo(ui, "Current-month tab:", "clm_curr", curr_title, titles.clone(), &mut |i| new_curr = i);
                    let opts: Vec<(usize, String)> = common.iter().cloned().enumerate().collect();
                    let mut key = self.cl_migration.key_col.clone();
                    let mut bal = self.cl_migration.bal_col.clone();
                    let mut cl = self.cl_migration.cl_col.clone();
                    let mut seg = self.cl_migration.segment_col.clone();
                    combo(ui, "Account key:", "clm_key", key.clone(), opts.clone(), &mut |i| key = common[i].clone());
                    combo(ui, "Balance:", "clm_bal", bal.clone(), opts.clone(), &mut |i| bal = common[i].clone());
                    combo(ui, "CL status:", "clm_cl", cl.clone(), opts.clone(), &mut |i| cl = common[i].clone());
                    let mut seg_opts: Vec<(usize, String)> = vec![(usize::MAX, "(none)".to_string())];
                    seg_opts.extend(opts.iter().cloned());
                    let seg_shown = if seg.is_empty() { "(none)".to_string() } else { seg.clone() };
                    combo(ui, "Segment (optional):", "clm_seg", seg_shown, seg_opts, &mut |i| seg = if i == usize::MAX { String::new() } else { common[i].clone() });
                    self.cl_migration.prev_tab = new_prev;
                    self.cl_migration.curr_tab = new_curr;
                    self.cl_migration.key_col = key;
                    self.cl_migration.bal_col = bal;
                    self.cl_migration.cl_col = cl;
                    self.cl_migration.segment_col = seg;
                    ui.label("Show:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.cl_migration.measure, MigrationMeasure::Balance, "Balance (BDT)");
                        ui.radio_value(&mut self.cl_migration.measure, MigrationMeasure::Count, "Number of accounts");
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut self.cl_migration.pct_of_row, "As % of the previous grade's row total");
                    ui.end_row();
                });
                rects.grid = grid.response.rect;
                ui.add_space(theme::SPACE_S);
                let legend = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "Rows are last month's grade (STD, SMA, SS, DF, BL, in the order of cl_code_map.json) and New; columns are this month's grade and {}. An upgrade is shaded green, a downgrade amber. A balance is last month's for an account that was there, this month's for a new one, so \"{}\" adds up to last month's total.",
                                MIGRATION_CLOSED_COL, micro_parquet::banking::MIGRATION_PREV_TOTAL_ROW
                            ))
                            .color(tk.muted)
                            .size(theme::TEXT_LABEL),
                        )
                        .wrap(),
                    );
                    ui.add(egui::Label::new(egui::RichText::new(format!("With a segment there is a block for each segment and a last block \"{}\".", MIGRATION_ALL_SEGMENTS)).color(tk.muted).size(theme::TEXT_LABEL)).wrap());
                });
                rects.legend = legend.response.rect;
                if let Some(p) = &problem {
                    ui.add_space(theme::SPACE_S);
                    theme::error_label(ui, p);
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.tabs.len() >= 2 {
                        let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Build migration matrix")).inner;
                        rects.buttons.push(("Build migration matrix".to_string(), b.rect));
                        if b.clicked() {
                            build = true;
                        }
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.cl_migration.rects = rects;
        if build {
            if let Ok((title, source)) = self.cl_migration_source() {
                let formats = self.migration_formats();
                self.create_tab_from_source_with(title, source, Vec::new(), formats, None);
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }
}
