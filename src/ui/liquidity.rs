//! Banking ▸ Treasury & ALM ▸ "LCR / NSFR" (Phase 5 R6): a GL tab, a liquidity map file, and two
//! result tabs ("LCR/NSFR lines" and "LCR/NSFR summary").

use crate::ui::pick::{col_combo, field_label, guess_column, DialogRects};
use crate::ui::prelude::*;
use micro_parquet::liquidity::{export_unmapped_csv, list_maps, load_map, run_df, BalanceSign, LiqMap, LiqSpec, MaturityUnit, UnmappedError};

pub const LIQ_TITLE: &str = "LCR / NSFR";

pub struct LiqState {
    pub map_files: Vec<String>,
    pub map_sel: String,
    /// The map file read: its name and the result.
    pub map: Option<(String, Result<LiqMap, String>)>,
    pub gl_col: String,
    pub balance_col: String,
    pub sign: BalanceSign,
    pub maturity_col: String,
    pub maturity_unit: MaturityUnit,
    /// What the last Build found wrong (empty = nothing), and the unmapped GL codes with their balances.
    pub error: String,
    pub unmapped: Vec<(String, f64)>,
    pub rects: DialogRects,
}

impl Default for LiqState {
    fn default() -> Self {
        Self {
            map_files: Vec::new(),
            map_sel: String::new(),
            map: None,
            gl_col: String::new(),
            balance_col: String::new(),
            sign: BalanceSign::DebitPositive,
            maturity_col: String::new(),
            maturity_unit: MaturityUnit::Months,
            error: String::new(),
            unmapped: Vec::new(),
            rects: DialogRects::default(),
        }
    }
}

impl MicroParquetApp {
    pub(crate) fn open_liquidity(&mut self) {
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let s = &mut self.liq;
        s.map_files = list_maps();
        if !s.map_files.contains(&s.map_sel) {
            s.map_sel = s.map_files.first().cloned().unwrap_or_default();
        }
        s.map = None;
        s.gl_col = guess_column(&cols, &["gl_code", "glcode", "gl_no", "gl", "account_code", "ledger"]);
        s.balance_col = guess_column(&cols, &["balance", "amount", "closing"]);
        s.maturity_col = guess_column(&cols, &["residual", "remaining", "maturity"]);
        s.error.clear();
        s.unmapped.clear();
        self.active_tool = ActiveToolModal::Liquidity;
    }

    fn refresh_liq_map(&mut self) {
        let sel = self.liq.map_sel.clone();
        if sel.is_empty() || matches!(&self.liq.map, Some((n, _)) if *n == sel) {
            return;
        }
        let r = load_map(&sel).map_err(|e| format!("{:#}", e));
        self.liq.map = Some((sel, r));
    }

    /// What the dialog would build, or the first thing that is missing.
    pub(crate) fn liquidity_inputs(&self) -> Result<(LiqMap, String, LiqSpec), String> {
        let s = &self.liq;
        let (name, map) = match &s.map {
            Some((n, Ok(m))) => (n.clone(), m.clone()),
            Some((_, Err(e))) => return Err(e.clone()),
            None => return Err("Choose the liquidity map.".to_string()),
        };
        let tab = self.tabs.get(self.active_tab).ok_or_else(|| "Open the tab with the GL balances first.".to_string())?;
        let cols = tab.available_columns();
        let has = |c: &str| !c.trim().is_empty() && cols.iter().any(|x| x == c);
        if !has(&s.gl_col) {
            return Err("Choose the GL code column.".to_string());
        }
        if !has(&s.balance_col) {
            return Err("Choose the balance column.".to_string());
        }
        if !s.maturity_col.trim().is_empty() && !has(&s.maturity_col) {
            return Err("The residual maturity column is not in the tab any more. Choose it again, or choose (none).".to_string());
        }
        let spec = LiqSpec { gl_col: s.gl_col.clone(), balance_col: s.balance_col.clone(), sign: s.sign, maturity_col: if s.maturity_col.trim().is_empty() { None } else { Some(s.maturity_col.clone()) }, maturity_unit: s.maturity_unit };
        Ok((map, name, spec))
    }

    /// Runs the map over the active tab. On success, the two tabs; on failure the message (and the unmapped codes) stay in the dialog.
    fn build_liquidity(&mut self) -> bool {
        self.liq.error.clear();
        self.liq.unmapped.clear();
        let Ok((map, map_file, spec)) = self.liquidity_inputs() else {
            return false;
        };
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let (source, steps) = (tab.source.clone(), tab.steps.clone());
        let df = match micro_parquet::engine::compile_tab_lazy(&source, &steps, &self.paths.temp_dir).and_then(|lf| Ok(lf.collect()?)) {
            Ok(df) => df,
            Err(e) => {
                self.liq.error = format!("{:#}", e);
                return false;
            }
        };
        if let Err(e) = run_df(&df, &spec, &map, &map_file) {
            if let Some(u) = e.downcast_ref::<UnmappedError>() {
                self.liq.unmapped = u.codes.clone();
            }
            self.liq.error = format!("{:#}", e);
            return false;
        }
        let make = |view| TabSource::Liquidity { source: Box::new(source.clone()), steps: steps.clone(), spec: spec.clone(), map: Box::new(map.clone()), map_file: map_file.clone(), view };
        let money: HashMap<String, ColumnFormatSpec> = [("Value".to_string(), ColumnFormatSpec { fmt_type: MetricFormatType::StandardCommas, decimals: 2 })].into_iter().collect();
        let pct: HashMap<String, ColumnFormatSpec> = [("Factor %".to_string(), ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 2 })].into_iter().collect();
        self.create_tab_from_source_with("LCR/NSFR lines".to_string(), make(micro_parquet::models::LiquidityView::Lines), Vec::new(), pct, None);
        self.create_tab_from_source_with("LCR/NSFR summary".to_string(), make(micro_parquet::models::LiquidityView::Summary), Vec::new(), money, None);
        true
    }

    /// "Export unmapped": the GL codes the map does not cover, with their balances, as a CSV file.
    pub(crate) fn export_unmapped(&mut self) {
        if self.liq.unmapped.is_empty() {
            return;
        }
        let Some(path) = crate::ui::save_dialog::choose_save_path(&crate::ui::save_dialog::SaveSpec::new("CSV (*.csv)", "csv", "unmapped_gl_codes.csv")) else {
            return;
        };
        self.export_unmapped_to(path);
    }

    pub(crate) fn export_unmapped_to(&mut self, path: std::path::PathBuf) {
        match export_unmapped_csv(&self.liq.unmapped, &path) {
            Ok(()) => self.status_message = format!("Wrote {} unmapped GL code(s) to {:?}.", self.liq.unmapped.len(), path),
            Err(e) => self.status_message = format!("{:#}", e),
        }
    }

    pub(crate) fn render_liquidity_modal(&mut self, ctx: &egui::Context, cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut build = false;
        let mut export_unmapped = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  {}", theme::ph::CHART_BAR, LIQ_TITLE);
        let mut rects = DialogRects::default();
        self.refresh_liq_map();
        let problem = self.liquidity_inputs().err();
        let (sample, map_line) = match &self.liq.map {
            Some((n, Ok(m))) => (m.sample, format!("Map: {} (as of {}). {}", n, m.as_of, m.source)),
            _ => (false, String::new()),
        };
        theme::modal(&title).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.header(|ui| {
                if sample {
                    let r = ui.scope(|ui| theme::warning_label(ui, "Sample parameters: the liquidity map in use is marked sample (Basel factors), not Bangladesh Bank's. Replace its factors before the ratios go into a return."));
                    rects.banner = r.response.rect;
                }
                if let Some(p) = &problem {
                    theme::error_label(ui, p);
                }
                if !self.liq.error.is_empty() {
                    theme::error_label(ui, &self.liq.error.clone());
                }
            });
            d.body_scroll(|ui| {
                let s = &mut self.liq;
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Liquidity map");
                    let files = s.map_files.clone();
                    let r = egui::ComboBox::from_id_salt("liq_map").width(theme::LIQ_MAP_COMBO_W).selected_text(if s.map_sel.is_empty() { "Choose\u{2026}".to_string() } else { s.map_sel.clone() }).show_ui(ui, |ui| {
                        for f in &files {
                            ui.selectable_value(&mut s.map_sel, f.clone(), f.as_str());
                        }
                    });
                    rects.controls.push(("Liquidity map".to_string(), r.response.rect));
                });
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "GL code column");
                    let r = col_combo(ui, "liq_gl", &mut s.gl_col, &cols, false);
                    rects.controls.push(("GL code column".to_string(), r));
                    field_label(ui, &tk, "Balance column");
                    let r = col_combo(ui, "liq_bal", &mut s.balance_col, &cols, false);
                    rects.controls.push(("Balance column".to_string(), r));
                });
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Balance sign");
                    let r = egui::ComboBox::from_id_salt("liq_sign").width(theme::LIQ_SIGN_COMBO_W).selected_text(s.sign.label()).show_ui(ui, |ui| {
                        for v in [BalanceSign::DebitPositive, BalanceSign::NormalSide] {
                            ui.selectable_value(&mut s.sign, v, v.label());
                        }
                    });
                    rects.controls.push(("Balance sign".to_string(), r.response.rect));
                });
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Residual maturity column (NSFR)");
                    let r = col_combo(ui, "liq_mat", &mut s.maturity_col, &cols, true);
                    rects.controls.push(("Residual maturity column".to_string(), r));
                    field_label(ui, &tk, "in");
                    let r = egui::ComboBox::from_id_salt("liq_unit").width(theme::CF_DEFAULT_W).selected_text(s.maturity_unit.label()).show_ui(ui, |ui| {
                        for u in MaturityUnit::ALL {
                            ui.selectable_value(&mut s.maturity_unit, u, u.label());
                        }
                    });
                    rects.controls.push(("Maturity unit".to_string(), r.response.rect));
                });
                ui.add_space(theme::SPACE_S);
                if !s.unmapped.is_empty() {
                    let b = ui.button(format!("Export unmapped ({})", s.unmapped.len()));
                    rects.controls.push(("Export unmapped".to_string(), b.rect));
                    if b.clicked() {
                        export_unmapped = true;
                    }
                    ui.add_space(theme::SPACE_S);
                }
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: &str| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    if !map_line.is_empty() {
                        ui.add(muted(&map_line));
                    }
                    ui.add(muted("Each GL balance goes to the lines of the map by its rules (exact, prefix or range) and is turned so that its normal side is positive. HQLA is capped as in Basel (Level 2 at 40%, Level 2B at 15% of HQLA, from the map); net cash outflows are the outflows less the inflows, up to 75% of the outflows; LCR is HQLA over net cash outflows and NSFR the available over the required stable funding. A GL code that is not in the map stops the run. Amounts are BDT."));
                });
                rects.notes = notes.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Calculate LCR and NSFR")).inner;
                    rects.buttons.push(("Calculate LCR and NSFR".to_string(), b.rect));
                    if b.clicked() {
                        build = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.liq.rects = rects;
        if export_unmapped {
            self.export_unmapped();
        }
        if build && self.build_liquidity() {
            close = true;
        }
        ModalOutcome { close, step: None }
    }
}
