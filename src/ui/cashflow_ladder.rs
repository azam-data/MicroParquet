//! The IRRBB cash-flow ladder dialog (Phase 5 R1): choose the tabs that hold assets and liabilities,
//! say which column holds what, pick the cutoff date, the cash-flow mode and an optional
//! behaviour profile, and build the ladder (19 buckets by currency) in a new tab.
//!
//! The lines (one per tab) are shared with the IRRBB six-scenario dialog (R3).

use crate::ui::gap_ladder::RoleChoice;
use crate::ui::prelude::*;
use micro_parquet::banking::GapRole;
use micro_parquet::behaviour::{list_behaviour_files, load_behaviour, BehaviourFile};
use micro_parquet::cashflow::{CashFlowMode, CfInputSpec, Frequency, RateType, Repayment, Scenario};

/// One tab of an IRRBB tool: which column holds which role. An empty text is "none".
#[derive(Clone, Debug)]
pub struct CfLine {
    pub tab: usize,
    pub role: RoleChoice,
    pub side_col: String,
    pub principal_col: String,
    pub maturity_col: String,
    pub rate_col: String,
    pub reprice_col: String,
    pub rate_type_col: String,
    pub default_rate_type: RateType,
    pub repay_col: String,
    pub default_repayment: Repayment,
    pub freq_col: String,
    pub default_frequency: Frequency,
    pub product_col: String,
    pub currency_col: String,
    pub default_currency: String,
}

fn guess(columns: &[String], words: &[&str]) -> String {
    columns.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned().unwrap_or_default()
}

impl CfLine {
    /// A line for tab `tab` with columns, the columns guessed from their names.
    pub fn for_columns(tab: usize, cols: &[String]) -> CfLine {
        let side = guess(cols, &["side", "direction", "al_type", "asset_liab"]);
        CfLine {
            tab,
            role: if side.is_empty() { RoleChoice::Asset } else { RoleChoice::Side },
            side_col: side,
            principal_col: guess(cols, &["balance", "outstanding", "amount"]),
            maturity_col: guess(cols, &["matdat", "maturity", "mat_date"]),
            rate_col: guess(cols, &["rate", "roi", "interest"]),
            reprice_col: guess(cols, &["reprice", "repricing", "reset"]),
            rate_type_col: guess(cols, &["rate_type", "ratetype", "fixed_float", "fix_flt"]),
            default_rate_type: RateType::Fixed,
            repay_col: guess(cols, &["repay", "amortis", "amortiz"]),
            default_repayment: Repayment::Bullet,
            freq_col: guess(cols, &["freq", "installment_type", "instalment_type"]),
            default_frequency: Frequency::AtMaturity,
            product_col: guess(cols, &["product", "scheme", "prod_code"]),
            currency_col: guess(cols, &["currency", "ccy", "curr"]),
            default_currency: "BDT".to_string(),
        }
    }

    /// The columns of a tab as the IRRBB tools read them, or the first thing that is missing.
    pub fn spec(&self, tab_title: &str, cols: &[String], n: usize) -> Result<CfInputSpec, String> {
        let has = |c: &String| !c.trim().is_empty() && cols.contains(c);
        let opt = |c: &String| -> Result<Option<String>, String> {
            if c.trim().is_empty() {
                Ok(None)
            } else if cols.contains(c) {
                Ok(Some(c.clone()))
            } else {
                Err(format!("Line {} ({}): the column '{}' is not in the tab any more. Choose it again.", n, tab_title, c))
            }
        };
        let mut need: Vec<(&str, &String)> = vec![("outstanding amount", &self.principal_col), ("maturity date", &self.maturity_col)];
        if self.role == RoleChoice::Side {
            need.push(("side", &self.side_col));
        }
        for (what, c) in need {
            if !has(c) {
                return Err(format!("Line {} ({}): choose the {} column.", n, tab_title, what));
            }
        }
        if self.default_currency.trim().is_empty() && self.currency_col.trim().is_empty() {
            return Err(format!("Line {} ({}): type the currency (for example BDT) or choose a currency column.", n, tab_title));
        }
        Ok(CfInputSpec {
            role: match self.role {
                RoleChoice::Asset => GapRole::Asset,
                RoleChoice::Liability => GapRole::Liability,
                RoleChoice::Side => GapRole::Side(self.side_col.clone()),
            },
            principal_col: self.principal_col.clone(),
            rate_col: opt(&self.rate_col)?,
            rate_type_col: opt(&self.rate_type_col)?,
            default_rate_type: self.default_rate_type,
            maturity_col: self.maturity_col.clone(),
            reprice_col: opt(&self.reprice_col)?,
            repay_col: opt(&self.repay_col)?,
            default_repayment: self.default_repayment,
            freq_col: opt(&self.freq_col)?,
            default_frequency: self.default_frequency,
            product_col: opt(&self.product_col)?,
            currency_col: opt(&self.currency_col)?,
            default_currency: self.default_currency.trim().to_uppercase(),
        })
    }
}

/// What the lines drew, for the render tests.
#[derive(Clone, Debug)]
pub struct CfLineRects {
    /// The card of each line.
    pub cards: Vec<egui::Rect>,
    /// (line, control name, rect).
    pub controls: Vec<(usize, String, egui::Rect)>,
    pub add_button: egui::Rect,
}

impl Default for CfLineRects {
    fn default() -> Self {
        Self { cards: Vec::new(), controls: Vec::new(), add_button: egui::Rect::NOTHING }
    }
}

/// The tabs a line can point at: their titles and columns.
pub struct CfTabs<'a> {
    pub titles: &'a [String],
    pub columns: &'a [Vec<String>],
}

fn col_combo(ui: &mut egui::Ui, id: (&str, usize), value: &mut String, cols: &[String], optional: bool, enabled: bool) -> egui::Rect {
    let shown = if !enabled {
        "\u{2014}".to_string()
    } else if value.trim().is_empty() {
        if optional { "(none)".to_string() } else { "Choose\u{2026}".to_string() }
    } else {
        value.clone()
    };
    let r = ui
        .add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt(id).width(theme::CF_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                if optional {
                    ui.selectable_value(value, String::new(), "(none)");
                }
                for c in cols {
                    ui.selectable_value(value, c.clone(), c.as_str());
                }
            })
        })
        .inner;
    r.response.rect
}

/// Draws the lines (one card per tab). `add_text` is the label of the "add" button's tab.
pub fn cf_lines_ui(ui: &mut egui::Ui, lines: &mut Vec<CfLine>, tabs: &CfTabs, rects: &mut CfLineRects) {
    let tk = *theme::tokens(ui.visuals().dark_mode);
    let mut remove: Option<usize> = None;
    // the button is above the cards, so it is in view without scrolling past them
    let add = ui.button(format!("{}  Add tab", theme::ph::PLUS));
    rects.add_button = add.rect;
    for i in 0..lines.len() {
        let mut line = lines[i].clone();
        let cols: Vec<String> = tabs.columns.get(line.tab).cloned().unwrap_or_default();
        let before_tab = line.tab;
        let heading = format!("Tab {}", i + 1);
        let card = theme::card(ui, &heading, |ui| {
            let mut push = |name: &str, r: egui::Rect| rects.controls.push((i, name.to_string(), r));
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Tab").color(tk.muted).size(theme::TEXT_LABEL));
                let r = egui::ComboBox::from_id_salt(("cf_tab", i)).width(theme::GAP_COMBO_W).selected_text(tabs.titles.get(line.tab).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                    for (t, name) in tabs.titles.iter().enumerate() {
                        ui.selectable_value(&mut line.tab, t, name);
                    }
                });
                push("Tab", r.response.rect);
                ui.label(egui::RichText::new("Counts as").color(tk.muted).size(theme::TEXT_LABEL));
                let r = egui::ComboBox::from_id_salt(("cf_role", i)).width(theme::GAP_COMBO_W).selected_text(line.role.label()).show_ui(ui, |ui| {
                    for c in [RoleChoice::Asset, RoleChoice::Liability, RoleChoice::Side] {
                        ui.selectable_value(&mut line.role, c, c.label());
                    }
                });
                push("Counts as", r.response.rect);
                ui.label(egui::RichText::new("Side column").color(tk.muted).size(theme::TEXT_LABEL));
                let enabled = line.role == RoleChoice::Side;
                let r = col_combo(ui, ("cf_side", i), &mut line.side_col, &cols, false, enabled);
                push("Side column", r);
                let x = ui.add(egui::Button::new(egui::RichText::new(theme::ph::X).color(tk.muted)).frame(false)).on_hover_text("Remove this tab");
                push("Remove", x.rect);
                if x.clicked() {
                    remove = Some(i);
                }
            });
            ui.add_space(theme::SPACE_XS);
            egui::Grid::new(("cf_roles", i)).num_columns(6).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                for h in ["Role", "Column", "If no column", "Role", "Column", "If no column"] {
                    ui.label(egui::RichText::new(h).color(tk.muted).size(theme::TEXT_LABEL));
                }
                ui.end_row();
                // outstanding | maturity
                ui.label("Outstanding amount");
                let r = col_combo(ui, ("cf_prin", i), &mut line.principal_col, &cols, false, true);
                push("Outstanding amount", r);
                ui.label("");
                ui.label("Maturity date");
                let r = col_combo(ui, ("cf_mat", i), &mut line.maturity_col, &cols, false, true);
                push("Maturity date", r);
                ui.label("");
                ui.end_row();
                // rate | next repricing date
                ui.label("Rate (%)");
                let r = col_combo(ui, ("cf_rate", i), &mut line.rate_col, &cols, true, true);
                push("Rate", r);
                ui.label("");
                ui.label("Next repricing date");
                let r = col_combo(ui, ("cf_rep", i), &mut line.reprice_col, &cols, true, true);
                push("Next repricing date", r);
                ui.label("");
                ui.end_row();
                // rate type | repayment type
                ui.label("Rate type");
                let r = col_combo(ui, ("cf_rtype", i), &mut line.rate_type_col, &cols, true, true);
                push("Rate type", r);
                let r = egui::ComboBox::from_id_salt(("cf_rtype_def", i)).width(theme::CF_DEFAULT_W).selected_text(line.default_rate_type.label()).show_ui(ui, |ui| {
                    for v in [RateType::Fixed, RateType::Floating] {
                        ui.selectable_value(&mut line.default_rate_type, v, v.label());
                    }
                });
                push("Rate type default", r.response.rect);
                ui.label("Repayment type");
                let r = col_combo(ui, ("cf_repay", i), &mut line.repay_col, &cols, true, true);
                push("Repayment type", r);
                let r = egui::ComboBox::from_id_salt(("cf_repay_def", i)).width(theme::CF_DEFAULT_W).selected_text(line.default_repayment.label()).show_ui(ui, |ui| {
                    for v in Repayment::ALL {
                        ui.selectable_value(&mut line.default_repayment, v, v.label());
                    }
                });
                push("Repayment default", r.response.rect);
                ui.end_row();
                // frequency | product code
                ui.label("Frequency");
                let r = col_combo(ui, ("cf_freq", i), &mut line.freq_col, &cols, true, true);
                push("Frequency", r);
                let r = egui::ComboBox::from_id_salt(("cf_freq_def", i)).width(theme::CF_DEFAULT_W).selected_text(line.default_frequency.label()).show_ui(ui, |ui| {
                    for v in Frequency::ALL {
                        ui.selectable_value(&mut line.default_frequency, v, v.label());
                    }
                });
                push("Frequency default", r.response.rect);
                ui.label("Product code");
                let r = col_combo(ui, ("cf_prod", i), &mut line.product_col, &cols, true, true);
                push("Product code", r);
                ui.label("");
                ui.end_row();
                // currency
                ui.label("Currency");
                let r = col_combo(ui, ("cf_cur", i), &mut line.currency_col, &cols, true, true);
                push("Currency", r);
                let r = ui.add(egui::TextEdit::singleline(&mut line.default_currency).desired_width(theme::CF_DEFAULT_W));
                push("Currency default", r.rect);
                ui.end_row();
            });
        });
        rects.cards.push(card);
        if line.tab != before_tab {
            // another tab: its columns are guessed again
            let side_was = line.role;
            let fresh = CfLine::for_columns(line.tab, tabs.columns.get(line.tab).map(|c| c.as_slice()).unwrap_or(&[]));
            line = CfLine { role: if side_was == RoleChoice::Side && fresh.side_col.is_empty() { fresh.role } else { fresh.role }, ..fresh };
        }
        lines[i] = line;
    }
    if let Some(i) = remove {
        lines.remove(i);
    }
    if add.clicked() {
        let next = (lines.last().map(|l| l.tab + 1).unwrap_or(0)).min(tabs.titles.len().saturating_sub(1));
        let mut line = CfLine::for_columns(next, tabs.columns.get(next).map(|c| c.as_slice()).unwrap_or(&[]));
        if line.role != RoleChoice::Side {
            line.role = RoleChoice::Liability;
        }
        lines.push(line);
    }
}

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct CfLadderRects {
    pub lines: CfLineRects,
    pub settings: Vec<(String, egui::Rect)>,
    pub notes: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
}

impl Default for CfLadderRects {
    fn default() -> Self {
        Self { lines: CfLineRects::default(), settings: Vec::new(), notes: egui::Rect::NOTHING, buttons: Vec::new() }
    }
}

pub struct CfLadderState {
    pub lines: Vec<CfLine>,
    pub cutoff: String,
    pub mode: CashFlowMode,
    pub scenario: Scenario,
    /// The behaviour files in `portable_data/behaviour`, newest first.
    pub behaviour_files: Vec<String>,
    /// The file chosen (empty = none) and what reading it gave.
    pub behaviour_sel: String,
    pub behaviour: Option<(String, Result<BehaviourFile, String>)>,
    pub rects: CfLadderRects,
}

impl Default for CfLadderState {
    fn default() -> Self {
        Self { lines: Vec::new(), cutoff: String::new(), mode: CashFlowMode::PrincipalAndInterest, scenario: Scenario::Base, behaviour_files: Vec::new(), behaviour_sel: String::new(), behaviour: None, rects: CfLadderRects::default() }
    }
}

/// The last day of the month before `today`, as YYYY-MM-DD.
pub fn last_month_end(today: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    let first = today.with_day(1).unwrap_or(today);
    (first - chrono::Duration::days(1)).format("%Y-%m-%d").to_string()
}

/// Reads the chosen behaviour file again when the choice changed.
pub fn refresh_behaviour(sel: &str, cache: &mut Option<(String, Result<BehaviourFile, String>)>) {
    if sel.is_empty() {
        *cache = None;
        return;
    }
    if matches!(cache, Some((n, _)) if n == sel) {
        return;
    }
    let r = load_behaviour(&micro_parquet::rules::behaviour_dir(), sel).map(|f| f.data).map_err(|e| format!("{:#}", e));
    *cache = Some((sel.to_string(), r));
}

impl MicroParquetApp {
    /// Opens the dialog with the open tab as its first line.
    pub(crate) fn open_cf_ladder(&mut self) {
        if self.cf.cutoff.trim().is_empty() {
            self.cf.cutoff = last_month_end(chrono::Local::now().date_naive());
        }
        self.cf.behaviour_files = list_behaviour_files(&micro_parquet::rules::behaviour_dir());
        if !self.cf.behaviour_sel.is_empty() && !self.cf.behaviour_files.contains(&self.cf.behaviour_sel) {
            self.cf.behaviour_sel.clear();
        }
        self.cf.behaviour = None;
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        self.cf.lines = vec![CfLine::for_columns(self.active_tab, &cols)];
        self.active_tool = ActiveToolModal::IrrbbLadder;
    }

    /// The tab inputs of the lines, or the first thing that is missing.
    pub(crate) fn cf_inputs(&self, lines: &[CfLine]) -> Result<Vec<CfTabInput>, String> {
        if self.tabs.is_empty() {
            return Err("Open a tab with assets or liabilities first.".to_string());
        }
        if lines.is_empty() {
            return Err("Add at least one tab (assets or liabilities).".to_string());
        }
        let mut inputs = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let Some(tab) = self.tabs.get(l.tab) else {
                return Err(format!("Line {}: that tab no longer exists. Choose it again.", i + 1));
            };
            let spec = l.spec(&tab.title, &tab.available_columns(), i + 1)?;
            inputs.push(CfTabInput { source: tab.source.clone(), steps: tab.steps.clone(), spec });
        }
        Ok(inputs)
    }

    /// What the dialog would build, or the first thing that is missing.
    pub(crate) fn cf_ladder_source(&self) -> Result<(String, TabSource), String> {
        let g = &self.cf;
        let inputs = self.cf_inputs(&g.lines)?;
        if chrono::NaiveDate::parse_from_str(g.cutoff.trim(), "%Y-%m-%d").is_err() {
            return Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string());
        }
        let behaviour = match &g.behaviour {
            Some((n, Ok(f))) if *n == g.behaviour_sel => Some((n.clone(), f.clone())),
            Some((n, Err(e))) if *n == g.behaviour_sel => return Err(e.clone()),
            _ => None,
        };
        if !g.behaviour_sel.is_empty() && behaviour.is_none() {
            return Err("The behaviour profile has not been read yet.".to_string());
        }
        let title = "IRRBB cash-flow ladder".to_string();
        Ok((title, TabSource::IrrbbLadder { inputs, cutoff_date: g.cutoff.trim().to_string(), mode: g.mode, scenario: g.scenario, behaviour }))
    }

    pub(crate) fn render_cf_ladder_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut build = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  IRRBB cash-flow ladder", theme::ph::CHART_BAR);
        let mut rects = CfLadderRects::default();
        let sel = self.cf.behaviour_sel.clone();
        refresh_behaviour(&sel, &mut self.cf.behaviour);
        let problem = self.cf_ladder_source().err();
        theme::modal(&title).default_width(theme::CF_DIALOG_W).fill_height().show(ctx, |d| {
            d.header(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Cutoff date (YYYY-MM-DD)").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.cf.cutoff).desired_width(theme::CF_DEFAULT_W));
                    rects.settings.push(("Cutoff date".to_string(), r.rect));
                    ui.label(egui::RichText::new("Cash-flow mode").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = egui::ComboBox::from_id_salt("cf_mode").width(theme::CF_WIDE_W).selected_text(self.cf.mode.label()).show_ui(ui, |ui| {
                        for m in [CashFlowMode::PrincipalAndInterest, CashFlowMode::PrincipalOnly] {
                            ui.selectable_value(&mut self.cf.mode, m, m.label());
                        }
                    });
                    rects.settings.push(("Cash-flow mode".to_string(), r.response.rect));
                    ui.label(egui::RichText::new("Scenario").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = egui::ComboBox::from_id_salt("cf_scenario").width(theme::CF_DEFAULT_W).selected_text(self.cf.scenario.label()).show_ui(ui, |ui| {
                        for s in Scenario::ALL {
                            ui.selectable_value(&mut self.cf.scenario, s, s.label());
                        }
                    });
                    rects.settings.push(("Scenario".to_string(), r.response.rect));
                    ui.label(egui::RichText::new("Behaviour profile").color(tk.muted).size(theme::TEXT_LABEL));
                    let files = self.cf.behaviour_files.clone();
                    let shown = if self.cf.behaviour_sel.is_empty() { "(none)".to_string() } else { self.cf.behaviour_sel.clone() };
                    let r = egui::ComboBox::from_id_salt("cf_behaviour").width(theme::CF_WIDE_W).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.cf.behaviour_sel, String::new(), "(none)");
                        for f in &files {
                            ui.selectable_value(&mut self.cf.behaviour_sel, f.clone(), f.as_str());
                        }
                    });
                    rects.settings.push(("Behaviour profile".to_string(), r.response.rect));
                });
            });
            if let Some(p) = &problem {
                // always in view: a problem in the lines below is not hidden by the scroll
                d.header(|ui| theme::error_label(ui, p));
            }
            d.body_scroll(|ui| {
                if self.tabs.is_empty() {
                    ui.label("Open a tab with assets or liabilities first.");
                    return;
                }
                let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
                let columns: Vec<Vec<String>> = self.tabs.iter().map(|t| t.available_columns()).collect();
                let mut lines = std::mem::take(&mut self.cf.lines);
                cf_lines_ui(ui, &mut lines, &CfTabs { titles: &titles, columns: &columns }, &mut rects.lines);
                self.cf.lines = lines;
                ui.add_space(theme::SPACE_S);
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: &str| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    ui.add(muted("Each row gets dated cash flows from the cutoff date, slotted into the 19 buckets of the Bangladesh Bank IRRBB guideline (Table 3) by years from the cutoff (days \u{f7} 365). Assets and inflows are positive, liabilities and outflows negative. Amounts are taken as BDT or BDT-equivalent: there is no currency conversion."));
                    ui.add(muted("Interest of a period is the outstanding \u{d7} rate \u{f7} payments per year; a full period is used for the first period too. A floating row has one flow on its next repricing date. A matured liability goes to O/N; a matured asset is left out and counted in the last row."));
                    ui.add(muted("With a behaviour profile (Banking \u{25b8} Treasury & ALM \u{25b8} Behaviour profile), non-maturity deposits put their core part at the average maturity and the rest in O/N (principal only), and loans and term deposits use their prepayment (CPR) and early-redemption (TDRR) rates, scaled by the scenario."));
                });
                rects.notes = notes.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if !self.tabs.is_empty() {
                        let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Build cash-flow ladder")).inner;
                        rects.buttons.push(("Build cash-flow ladder".to_string(), b.rect));
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
        self.cf.rects = rects;
        if build {
            if let Ok((title, source)) = self.cf_ladder_source() {
                self.create_tab_from_source_with(title, source, Vec::new(), HashMap::new(), None);
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }
}
