//! The IRRBB (six scenarios) dialog (Phase 5 R3): the tabs and their columns (as in the cash-flow
//! ladder), the cutoff date, Tier 1 capital, the curve compounding, the optional post-shock floor,
//! behaviour profile and betas, and two result tabs: "IRRBB results" and "IRRBB cash flows".

use crate::ui::cashflow_ladder::{cf_lines_ui, last_month_end, refresh_behaviour, CfLine, CfLineRects, CfTabs};
use crate::ui::prelude::*;
use micro_parquet::behaviour::{list_behaviour_files, BehaviourFile};
use micro_parquet::betas::{betas_dir, list_beta_files, load_beta_rows};
use micro_parquet::cashflow::CashFlowMode;
use micro_parquet::curves::{curves_dir, CurveStore};
use micro_parquet::irrbb::{Compounding, IrrbbSpec};
use micro_parquet::rules::{load_shocks, RuleFile, ShockFile};

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct IrrbbRects {
    pub lines: CfLineRects,
    pub header: Vec<(String, egui::Rect)>,
    pub banner: egui::Rect,
    pub notes: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
}

impl Default for IrrbbRects {
    fn default() -> Self {
        Self { lines: CfLineRects::default(), header: Vec::new(), banner: egui::Rect::NOTHING, notes: egui::Rect::NOTHING, buttons: Vec::new() }
    }
}

pub struct IrrbbState {
    pub lines: Vec<CfLine>,
    pub cutoff: String,
    pub mode: CashFlowMode,
    /// "Treat blank maturity as O/N".
    pub blank_on: bool,
    pub tier1: String,
    pub compounding: Compounding,
    pub post_floor_on: bool,
    pub post_floor: String,
    pub nii_floor_on: bool,
    pub nii_floor: String,
    pub behaviour_files: Vec<String>,
    pub behaviour_sel: String,
    pub behaviour: Option<(String, Result<BehaviourFile, String>)>,
    pub beta_files: Vec<String>,
    pub beta_sel: String,
    /// The shock rule file in force at the cutoff date: the date it was read for and the result.
    pub shocks: Option<(String, Result<RuleFile<ShockFile>, String>)>,
    pub rects: IrrbbRects,
}

impl Default for IrrbbState {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            cutoff: String::new(),
            mode: CashFlowMode::PrincipalAndInterest,
            blank_on: false,
            tier1: String::new(),
            compounding: Compounding::Annual,
            post_floor_on: false,
            post_floor: "0".to_string(),
            nii_floor_on: false,
            nii_floor: "0".to_string(),
            behaviour_files: Vec::new(),
            behaviour_sel: String::new(),
            behaviour: None,
            beta_files: Vec::new(),
            beta_sel: String::new(),
            shocks: None,
            rects: IrrbbRects::default(),
        }
    }
}

fn parse_amount(t: &str) -> Option<f64> {
    t.trim().replace(',', "").parse::<f64>().ok().filter(|v| v.is_finite())
}

impl MicroParquetApp {
    pub(crate) fn open_irrbb(&mut self) {
        if self.irrbb.cutoff.trim().is_empty() {
            self.irrbb.cutoff = last_month_end(chrono::Local::now().date_naive());
        }
        self.irrbb.behaviour_files = list_behaviour_files(&micro_parquet::rules::behaviour_dir());
        if !self.irrbb.behaviour_files.contains(&self.irrbb.behaviour_sel) {
            self.irrbb.behaviour_sel.clear();
        }
        self.irrbb.beta_files = list_beta_files(&betas_dir());
        if !self.irrbb.beta_files.contains(&self.irrbb.beta_sel) {
            self.irrbb.beta_sel.clear();
        }
        self.irrbb.behaviour = None;
        self.irrbb.shocks = None;
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        self.irrbb.lines = vec![CfLine::for_columns(self.active_tab, &cols)];
        self.active_tool = ActiveToolModal::IrrbbSix;
    }

    /// Reads the shock file in force at the cutoff date again when the date changed.
    fn refresh_irrbb_shocks(&mut self) {
        let key = self.irrbb.cutoff.trim().to_string();
        if matches!(&self.irrbb.shocks, Some((k, _)) if *k == key) {
            return;
        }
        let r = match chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d") {
            Ok(d) => load_shocks(d).map_err(|e| format!("{:#}", e)),
            Err(_) => Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string()),
        };
        self.irrbb.shocks = Some((key, r));
    }

    /// The two tabs the dialog would build ("IRRBB results", then "IRRBB cash flows"), or the first thing that is missing.
    pub(crate) fn irrbb_sources(&self) -> Result<Vec<(String, TabSource)>, String> {
        let g = &self.irrbb;
        let inputs = self.cf_inputs(&g.lines)?;
        let cutoff = chrono::NaiveDate::parse_from_str(g.cutoff.trim(), "%Y-%m-%d").map_err(|_| "Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string())?;
        let tier1 = parse_amount(&g.tier1).filter(|v| *v > 0.0).ok_or_else(|| "Type the Tier 1 capital in BDT (a number above 0): the outlier test is a percentage of it.".to_string())?;
        let shocks = match &g.shocks {
            Some((k, Ok(f))) if *k == g.cutoff.trim() => f.clone(),
            Some((k, Err(e))) if *k == g.cutoff.trim() => return Err(e.clone()),
            _ => return Err("The shock file has not been read yet.".to_string()),
        };
        let floor = |on: bool, text: &str, what: &str| -> Result<Option<f64>, String> {
            if !on {
                return Ok(None);
            }
            parse_amount(text).map(Some).ok_or_else(|| format!("Type the {} floor as a number in percent (for example 0).", what))
        };
        let post_floor = floor(g.post_floor_on, &g.post_floor, "post-shock")?;
        let nii_floor = floor(g.nii_floor_on, &g.nii_floor, "\u{394}NII rate")?;
        let behaviour = if g.behaviour_sel.is_empty() {
            None
        } else {
            match &g.behaviour {
                Some((n, Ok(f))) if *n == g.behaviour_sel => Some((n.clone(), f.clone())),
                Some((n, Err(e))) if *n == g.behaviour_sel => return Err(e.clone()),
                _ => return Err("The behaviour profile has not been read yet.".to_string()),
            }
        };
        let betas = if g.beta_sel.is_empty() { Vec::new() } else { load_beta_rows(&betas_dir(), &g.beta_sel).map_err(|e| format!("{:#}", e))? };
        let store = CurveStore::load(&curves_dir());
        let curves = store.strict().map_err(|e| format!("{:#}", e))?.to_vec();
        let spec = IrrbbSpec { cutoff, mode: g.mode, blank_maturity_on: g.blank_on, tier1, compounding: g.compounding, post_shock_floor_pct: post_floor, nii_floor_pct: nii_floor, shocks, curves, behaviour, betas, beta_file: g.beta_sel.clone() };
        let make = |view| TabSource::Irrbb { inputs: inputs.clone(), spec: Box::new(spec.clone()), view };
        Ok(vec![("IRRBB cash flows".to_string(), make(IrrbbView::CashFlows)), ("IRRBB results".to_string(), make(IrrbbView::Results))])
    }

    fn irrbb_formats() -> HashMap<String, ColumnFormatSpec> {
        let pct = ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 };
        [("Loss % of Tier 1".to_string(), pct)].into_iter().collect()
    }

    pub(crate) fn render_irrbb_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut build = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  IRRBB (six scenarios)", theme::ph::CHART_LINE);
        let mut rects = IrrbbRects::default();
        self.refresh_irrbb_shocks();
        let sel = self.irrbb.behaviour_sel.clone();
        refresh_behaviour(&sel, &mut self.irrbb.behaviour);
        let problem = self.irrbb_sources().err();
        let sample = matches!(&self.irrbb.shocks, Some((_, Ok(f))) if f.meta.sample);
        let shock_line = match &self.irrbb.shocks {
            Some((_, Ok(f))) => {
                let sizes: Vec<String> = f.data.currencies.iter().map(|c| format!("{}: parallel {} bp, short {} bp, long {} bp", c.currency, c.parallel_bp, c.short_bp, c.long_bp)).collect();
                format!("Shocks: {} ({}). {}. Outlier test: {}% of Tier 1.", f.meta.file, f.meta.source, sizes.join("; "), f.data.outlier_pct)
            }
            _ => String::new(),
        };
        theme::modal(&title).default_width(theme::CF_DIALOG_W).fill_height().show(ctx, |d| {
            d.header(|ui| {
                if sample {
                    let r = ui.scope(|ui| theme::warning_label(ui, "Sample parameters: the shock file in use is marked sample. Confirm its numbers before they go into a return."));
                    rects.banner = r.response.rect;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Cutoff date (YYYY-MM-DD)").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.irrbb.cutoff).desired_width(theme::CF_DEFAULT_W));
                    rects.header.push(("Cutoff date".to_string(), r.rect));
                    ui.label(egui::RichText::new("Tier 1 capital (BDT)").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.irrbb.tier1).desired_width(theme::CF_DEFAULT_W));
                    rects.header.push(("Tier 1 capital".to_string(), r.rect));
                    ui.label(egui::RichText::new("Cash-flow mode").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = egui::ComboBox::from_id_salt("irrbb_mode").width(theme::CF_WIDE_W).selected_text(self.irrbb.mode.label()).show_ui(ui, |ui| {
                        for m in [CashFlowMode::PrincipalAndInterest, CashFlowMode::PrincipalOnly] {
                            ui.selectable_value(&mut self.irrbb.mode, m, m.label());
                        }
                    });
                    rects.header.push(("Cash-flow mode".to_string(), r.response.rect));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Curve rates are").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = egui::ComboBox::from_id_salt("irrbb_comp").width(theme::CF_COMBO_W * 2.0).selected_text(self.irrbb.compounding.label()).show_ui(ui, |ui| {
                        for c in [Compounding::Annual, Compounding::Continuous] {
                            ui.selectable_value(&mut self.irrbb.compounding, c, c.label());
                        }
                    });
                    rects.header.push(("Compounding".to_string(), r.response.rect));
                    let r = ui.checkbox(&mut self.irrbb.post_floor_on, "Post-shock floor (%)");
                    rects.header.push(("Post-shock floor".to_string(), r.rect));
                    let r = ui.add_enabled(self.irrbb.post_floor_on, egui::TextEdit::singleline(&mut self.irrbb.post_floor).desired_width(theme::BEHAVIOUR_CELL_W));
                    rects.header.push(("Post-shock floor value".to_string(), r.rect));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Behaviour profile").color(tk.muted).size(theme::TEXT_LABEL));
                    let files = self.irrbb.behaviour_files.clone();
                    let shown = if self.irrbb.behaviour_sel.is_empty() { "(none)".to_string() } else { self.irrbb.behaviour_sel.clone() };
                    let r = egui::ComboBox::from_id_salt("irrbb_behaviour").width(theme::CF_WIDE_W).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.irrbb.behaviour_sel, String::new(), "(none)");
                        for f in &files {
                            ui.selectable_value(&mut self.irrbb.behaviour_sel, f.clone(), f.as_str());
                        }
                    });
                    rects.header.push(("Behaviour profile".to_string(), r.response.rect));
                    let r = ui.checkbox(&mut self.irrbb.blank_on, "Treat blank maturity as O/N");
                    rects.header.push(("Blank maturity".to_string(), r.rect));
                    ui.label(egui::RichText::new("Beta file (\u{394}NII)").color(tk.muted).size(theme::TEXT_LABEL));
                    let files = self.irrbb.beta_files.clone();
                    let shown = if self.irrbb.beta_sel.is_empty() { "(none)".to_string() } else { self.irrbb.beta_sel.clone() };
                    let r = egui::ComboBox::from_id_salt("irrbb_beta").width(theme::CF_WIDE_W).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.irrbb.beta_sel, String::new(), "(none)");
                        for f in &files {
                            ui.selectable_value(&mut self.irrbb.beta_sel, f.clone(), f.as_str());
                        }
                    });
                    rects.header.push(("Beta file".to_string(), r.response.rect));
                    let r = ui.checkbox(&mut self.irrbb.nii_floor_on, "\u{394}NII rate floor (%)");
                    rects.header.push(("NII floor".to_string(), r.rect));
                    let r = ui.add_enabled(self.irrbb.nii_floor_on, egui::TextEdit::singleline(&mut self.irrbb.nii_floor).desired_width(theme::BEHAVIOUR_CELL_W));
                    rects.header.push(("NII floor value".to_string(), r.rect));
                });
                if let Some(p) = &problem {
                    theme::error_label(ui, p);
                }
            });
            d.body_scroll(|ui| {
                if self.tabs.is_empty() {
                    ui.label("Open a tab with assets or liabilities first.");
                    return;
                }
                let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
                let columns: Vec<Vec<String>> = self.tabs.iter().map(|t| t.available_columns()).collect();
                let mut lines = std::mem::take(&mut self.irrbb.lines);
                cf_lines_ui(ui, &mut lines, &CfTabs { titles: &titles, columns: &columns }, &mut rects.lines);
                self.irrbb.lines = lines;
                ui.add_space(theme::SPACE_S);
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: &str| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    if !shock_line.is_empty() {
                        ui.add(muted(&shock_line));
                    }
                    ui.add(muted("Six scenarios (BB \u{a7}14.2): parallel up and down, short up and down, steepener and flattener. EVE change = the present value of the net cash flow of each bucket, discounted at its midpoint with e^(\u{2212}R\u{b7}t), after the shock minus before it; R comes from the yield curve of the same currency dated on or before the cutoff (Banking \u{25b8} Treasury & ALM \u{25b8} Yield curves), linearly interpolated. Loss is the largest fall in EVE, as a percentage of Tier 1; a loss of 15% of Tier 1 or more makes the bank an outlier (BB \u{a7}13.1)."));
                    ui.add(muted("\u{394}NII uses the repricing date, the beta file (up and down) and the rate floor of the interest rate shock tool, with the scenario's shock at each row's repricing time, for the year ahead. Each currency runs with its own curve and shocks; the loss of a scenario adds each currency's loss with no offsets (BB \u{a7}15.32). Amounts are BDT or BDT-equivalent: there is no currency conversion."));
                });
                rects.notes = notes.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if !self.tabs.is_empty() {
                        let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Run IRRBB")).inner;
                        rects.buttons.push(("Run IRRBB".to_string(), b.rect));
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
        self.irrbb.rects = rects;
        if build {
            if let Ok(tabs) = self.irrbb_sources() {
                for (title, source) in tabs {
                    self.create_tab_from_source_with(title, source, Vec::new(), Self::irrbb_formats(), None);
                }
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }
}
