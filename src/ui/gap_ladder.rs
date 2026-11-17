//! The liquidity gap ladder dialog (Phase 4 V3): choose the tabs that hold assets (inflows) and
//! liabilities (outflows), their amount and maturity columns, a bucket preset and the cutoff date,
//! and build the ladder in a new tab.

use crate::ui::prelude::*;
use micro_parquet::banking::{gap_limits_path, load_gap_limits, GapInputSpec, GapLimitsFile, GapRole};

/// How one tab counts in the ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleChoice {
    Asset,
    Liability,
    Side,
}

impl RoleChoice {
    fn label(self) -> &'static str {
        match self {
            RoleChoice::Asset => "Asset / inflow",
            RoleChoice::Liability => "Liability / outflow",
            RoleChoice::Side => "Side column (A / L)",
        }
    }
}

/// One tab of the ladder.
#[derive(Clone, Debug)]
pub struct GapRow {
    pub tab: usize,
    pub role: RoleChoice,
    pub side_col: String,
    pub amount_col: String,
    pub maturity_col: String,
}

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct GapRects {
    pub grid: egui::Rect,
    pub row_controls: Vec<(usize, String, egui::Rect)>,
    pub settings: Vec<(String, egui::Rect)>,
    pub notes: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
}

impl Default for GapRects {
    fn default() -> Self {
        Self { grid: egui::Rect::NOTHING, row_controls: Vec::new(), settings: Vec::new(), notes: egui::Rect::NOTHING, buttons: Vec::new() }
    }
}

pub struct GapState {
    pub rows: Vec<GapRow>,
    pub presets: Vec<CustomBucketDef>,
    pub preset_idx: usize,
    pub cutoff: String,
    pub null_as_sight: bool,
    /// The limits file of the chosen preset: read again when the preset or the dialog changes.
    pub limits: Option<(usize, Result<Option<GapLimitsFile>, String>)>,
    pub rects: GapRects,
}

impl Default for GapState {
    fn default() -> Self {
        Self { rows: Vec::new(), presets: Vec::new(), preset_idx: 0, cutoff: String::new(), null_as_sight: true, limits: None, rects: GapRects::default() }
    }
}

/// The last day of the month before `today`, as YYYY-MM-DD.
fn last_month_end(today: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    let first = today.with_day(1).unwrap_or(today);
    (first - chrono::Duration::days(1)).format("%Y-%m-%d").to_string()
}

fn guess(columns: &[String], words: &[&str]) -> Option<String> {
    columns.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned()
}

impl MicroParquetApp {
    /// Opens the dialog with the open tab as its first line.
    pub(crate) fn open_gap_ladder(&mut self) {
        self.gap.presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
        if self.gap.preset_idx >= self.gap.presets.len() {
            self.gap.preset_idx = 0;
        }
        if self.gap.cutoff.trim().is_empty() {
            self.gap.cutoff = last_month_end(chrono::Local::now().date_naive());
        }
        self.gap.limits = None;
        self.gap.rows = vec![self.gap_row_for(self.active_tab)];
        self.active_tool = ActiveToolModal::GapLadder;
    }

    /// A line for tab `tab`, with the columns guessed from their names.
    fn gap_row_for(&self, tab: usize) -> GapRow {
        let cols = self.tabs.get(tab).map(|t| t.available_columns()).unwrap_or_default();
        let side = guess(&cols, &["side", "direction", "al_type", "asset_liab"]);
        GapRow {
            tab,
            role: if side.is_some() { RoleChoice::Side } else { RoleChoice::Asset },
            side_col: side.unwrap_or_default(),
            amount_col: guess(&cols, &["balance", "outstanding", "amount"]).unwrap_or_default(),
            maturity_col: guess(&cols, &["matdat", "maturity", "mat_date"]).unwrap_or_default(),
        }
    }

    /// What the dialog would build, or the first thing that is missing.
    pub(crate) fn gap_ladder_source(&self) -> Result<(String, TabSource), String> {
        let g = &self.gap;
        if self.tabs.is_empty() {
            return Err("Open a tab with assets or liabilities first.".to_string());
        }
        if g.rows.is_empty() {
            return Err("Add at least one tab (assets or liabilities).".to_string());
        }
        let Some(preset) = g.presets.get(g.preset_idx).cloned() else {
            return Err("Choose a bucket preset.".to_string());
        };
        if chrono::NaiveDate::parse_from_str(g.cutoff.trim(), "%Y-%m-%d").is_err() {
            return Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string());
        }
        let mut inputs = Vec::new();
        for (i, r) in g.rows.iter().enumerate() {
            let Some(tab) = self.tabs.get(r.tab) else {
                return Err(format!("Line {}: that tab no longer exists. Choose it again.", i + 1));
            };
            let cols = tab.available_columns();
            let mut need: Vec<(&str, &String)> = vec![("amount", &r.amount_col), ("maturity", &r.maturity_col)];
            if r.role == RoleChoice::Side {
                need.push(("side", &r.side_col));
            }
            for (what, c) in need {
                if c.trim().is_empty() || !cols.contains(c) {
                    return Err(format!("Line {} ({}): choose the {} column.", i + 1, tab.title, what));
                }
            }
            let role = match r.role {
                RoleChoice::Asset => GapRole::Asset,
                RoleChoice::Liability => GapRole::Liability,
                RoleChoice::Side => GapRole::Side(r.side_col.clone()),
            };
            inputs.push(GapTabInput { source: tab.source.clone(), steps: tab.steps.clone(), spec: GapInputSpec { role, amount_col: r.amount_col.clone(), maturity_col: r.maturity_col.clone() } });
        }
        let limits = match &g.limits {
            Some((idx, Ok(l))) if *idx == g.preset_idx => l.clone(),
            Some((idx, Err(e))) if *idx == g.preset_idx => return Err(e.clone()),
            _ => None,
        };
        let title = format!("Gap ladder ({})", preset.name);
        Ok((title, TabSource::GapLadder { inputs, preset, cutoff_date: g.cutoff.trim().to_string(), null_as_sight: g.null_as_sight, limits }))
    }

    fn refresh_gap_limits(&mut self) {
        if matches!(&self.gap.limits, Some((i, _)) if *i == self.gap.preset_idx) {
            return;
        }
        let Some(p) = self.gap.presets.get(self.gap.preset_idx) else {
            return;
        };
        let r = load_gap_limits(&self.paths.gap_limits_dir(), p).map_err(|e| format!("{:#}", e));
        self.gap.limits = Some((self.gap.preset_idx, r));
    }

    /// The formats of the ladder: the percentages as percentages.
    fn gap_formats(&self) -> HashMap<String, ColumnFormatSpec> {
        let pct = ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 };
        [micro_parquet::banking::GAP_COLUMNS[5], micro_parquet::banking::GAP_COLUMNS[6], micro_parquet::banking::GAP_LIMIT_COLUMN].iter().map(|n| (n.to_string(), pct)).collect()
    }

    pub(crate) fn render_gap_ladder_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut build = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  Liquidity gap ladder", theme::ph::CHART_BAR);
        let mut rects = GapRects::default();
        self.refresh_gap_limits();
        let problem = self.gap_ladder_source().err();
        theme::modal(&title).default_width(theme::GAP_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                if self.tabs.is_empty() {
                    ui.label("Open a tab with assets or liabilities first.");
                    return;
                }
                let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
                let tab_columns: Vec<Vec<String>> = self.tabs.iter().map(|t| t.available_columns()).collect();
                let mut remove: Option<usize> = None;
                let grid = egui::Grid::new("gap_rows").num_columns(6).spacing(egui::vec2(theme::SPACE_S, theme::SPACE_S)).show(ui, |ui| {
                    for h in ["Tab", "Counts as", "Side column", "Amount", "Maturity date", ""] {
                        ui.label(egui::RichText::new(h).color(tk.muted).size(theme::TEXT_LABEL));
                    }
                    ui.end_row();
                    for i in 0..self.gap.rows.len() {
                        let mut row = self.gap.rows[i].clone();
                        let cols = tab_columns.get(row.tab).cloned().unwrap_or_default();
                        let mut push = |name: &str, r: egui::Rect| rects.row_controls.push((i, name.to_string(), r));
                        let r = egui::ComboBox::from_id_salt(("gap_tab", i)).width(theme::GAP_COMBO_W).selected_text(titles.get(row.tab).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                            for (t, name) in titles.iter().enumerate() {
                                ui.selectable_value(&mut row.tab, t, name);
                            }
                        });
                        push("Tab", r.response.rect);
                        let r = egui::ComboBox::from_id_salt(("gap_role", i)).width(theme::GAP_COMBO_W).selected_text(row.role.label()).show_ui(ui, |ui| {
                            for c in [RoleChoice::Asset, RoleChoice::Liability, RoleChoice::Side] {
                                ui.selectable_value(&mut row.role, c, c.label());
                            }
                        });
                        push("Counts as", r.response.rect);
                        let col_combo = |ui: &mut egui::Ui, id: (&str, usize), value: &mut String, enabled: bool, push: &mut dyn FnMut(&str, egui::Rect), name: &str| {
                            let r = ui
                                .add_enabled_ui(enabled, |ui| {
                                    egui::ComboBox::from_id_salt(id).width(theme::GAP_COMBO_NARROW_W).selected_text(if enabled { value.clone() } else { "\u{2014}".to_string() }).show_ui(ui, |ui| {
                                        for c in &cols {
                                            ui.selectable_value(value, c.clone(), c.as_str());
                                        }
                                    })
                                })
                                .inner;
                            push(name, r.response.rect);
                        };
                        let enabled_side = row.role == RoleChoice::Side;
                        let mut side = row.side_col.clone();
                        col_combo(ui, ("gap_side", i), &mut side, enabled_side, &mut push, "Side column");
                        row.side_col = side;
                        let mut amt = row.amount_col.clone();
                        col_combo(ui, ("gap_amt", i), &mut amt, true, &mut push, "Amount");
                        row.amount_col = amt;
                        let mut mat = row.maturity_col.clone();
                        col_combo(ui, ("gap_mat", i), &mut mat, true, &mut push, "Maturity date");
                        row.maturity_col = mat;
                        let x = ui.add(egui::Button::new(egui::RichText::new(theme::ph::X).color(tk.muted)).frame(false)).on_hover_text("Remove this tab");
                        push("Remove", x.rect);
                        if x.clicked() {
                            remove = Some(i);
                        }
                        if row.tab != self.gap.rows[i].tab {
                            // another tab: its columns are guessed again
                            let fresh = self.gap_row_for(row.tab);
                            row.amount_col = fresh.amount_col;
                            row.maturity_col = fresh.maturity_col;
                            row.side_col = fresh.side_col;
                        }
                        self.gap.rows[i] = row;
                        ui.end_row();
                    }
                });
                rects.grid = grid.response.rect;
                if let Some(i) = remove {
                    self.gap.rows.remove(i);
                }
                if ui.button(format!("{}  Add tab", theme::ph::PLUS)).clicked() {
                    let next = (self.gap.rows.last().map(|r| r.tab + 1).unwrap_or(0)).min(self.tabs.len().saturating_sub(1));
                    let mut row = self.gap_row_for(next);
                    row.role = if row.role == RoleChoice::Side { RoleChoice::Side } else { RoleChoice::Liability };
                    self.gap.rows.push(row);
                }
                ui.add_space(theme::SPACE_S);
                egui::Grid::new("gap_settings").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    ui.label("Bucket preset:");
                    let names: Vec<String> = self.gap.presets.iter().map(|p| p.name.clone()).collect();
                    let r = egui::ComboBox::from_id_salt("gap_preset").width(theme::GAP_COMBO_W * 2.0).selected_text(names.get(self.gap.preset_idx).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut self.gap.preset_idx, i, n);
                        }
                    });
                    rects.settings.push(("Bucket preset".to_string(), r.response.rect));
                    ui.end_row();
                    ui.label("Cutoff date (YYYY-MM-DD):");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.gap.cutoff).desired_width(theme::GAP_COMBO_NARROW_W));
                    rects.settings.push(("Cutoff date".to_string(), r.rect));
                    ui.end_row();
                    ui.label("");
                    let r = ui.checkbox(&mut self.gap.null_as_sight, "Treat a blank maturity as sight (otherwise as the preset's long bucket)");
                    rects.settings.push(("Blank maturity".to_string(), r.rect));
                    ui.end_row();
                });
                ui.add_space(theme::SPACE_S);
                let limits_line = match &self.gap.limits {
                    Some((i, Ok(Some(l)))) if *i == self.gap.preset_idx => format!("Limits: {} bucket(s) from {}{}. A row whose cumulative gap % is below its limit is marked BREACH.", l.limits.len(), gap_limits_path(std::path::Path::new("gap_limits"), self.gap.presets.get(*i).map(|p| p.name.as_str()).unwrap_or("")).display(), if l.source.is_empty() { String::new() } else { format!(" ({})", l.source) }),
                    Some((i, Ok(None))) if *i == self.gap.preset_idx => format!("No limits file for this preset: add portable_data/{} to get a Limit and a Breach column.", gap_limits_path(std::path::Path::new("gap_limits"), self.gap.presets.get(*i).map(|p| p.name.as_str()).unwrap_or("<preset>")).display()),
                    _ => String::new(),
                };
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: String| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    ui.add(muted("Inflows are the amounts of asset rows and outflows those of liability rows, by the bucket of their maturity date. Percentages are of total assets (all inflows). Cumulative gap is the running sum of the net gap.".to_string()));
                    ui.add(muted("Behavioural treatment of current and savings accounts (core and volatile parts) comes in Phase 5. Until then a deposit with no maturity date goes to the sight bucket, or to the preset's long bucket if you untick the box above.".to_string()));
                    if !limits_line.is_empty() {
                        ui.add(muted(limits_line.clone()));
                    }
                });
                rects.notes = notes.response.rect;
                if let Some(p) = &problem {
                    ui.add_space(theme::SPACE_S);
                    theme::error_label(ui, p);
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if !self.tabs.is_empty() {
                        let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Build gap ladder")).inner;
                        rects.buttons.push(("Build gap ladder".to_string(), b.rect));
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
        self.gap.rects = rects;
        if build {
            if let Ok((title, source)) = self.gap_ladder_source() {
                let formats = self.gap_formats();
                self.create_tab_from_source_with(title, source, Vec::new(), formats, None);
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }
}
