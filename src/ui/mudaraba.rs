//! The Islamic window's Mudaraba profit distribution (Phase 6 P5): Banking ▸ Islamic window ▸
//! "Mudaraba profit distribution". A calculation only: the CRR, the depositors' PSR and the
//! weightages come from `portable_data/islamic/psr_<date>.json`, which the bank's Shariah
//! Supervisory Committee must have approved; a shipped file is a sample and the dialog says so.

use crate::ui::prelude::*;
use micro_parquet::islamic::{load_psr, MudarabaSpec, PsrFile, MUDARABA_COLUMNS};
use micro_parquet::rules::RuleFile;

#[derive(Clone, Debug, Default)]
pub struct MudRects {
    pub combos: Vec<(String, egui::Rect)>,
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub banner: Option<egui::Rect>,
    pub params: Option<egui::Rect>,
}

pub struct MudState {
    pub product_col: String,
    pub balance_col: String,
    /// The rows are daily balances (their sum ÷ the days) instead of an average balance.
    pub daily: bool,
    pub start: String,
    pub end: String,
    pub income: String,
    pub other: String,
    /// The parameter file in force at the end date: the date it was read for and the result.
    pub params: Option<(String, Result<RuleFile<PsrFile>, String>)>,
    pub rects: MudRects,
}

impl Default for MudState {
    fn default() -> Self {
        Self { product_col: String::new(), balance_col: String::new(), daily: false, start: String::new(), end: String::new(), income: String::new(), other: "0".to_string(), params: None, rects: MudRects::default() }
    }
}

fn number(t: &str) -> Option<f64> {
    t.trim().replace(',', "").parse::<f64>().ok().filter(|v| v.is_finite())
}

fn guess(cols: &[String], words: &[&str]) -> String {
    cols.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned().unwrap_or_default()
}

/// The first and last day of the month before `today`.
fn last_month(today: chrono::NaiveDate) -> (String, String) {
    use chrono::Datelike;
    let first_this = today.with_day(1).unwrap_or(today);
    let last = first_this - chrono::Duration::days(1);
    let first = last.with_day(1).unwrap_or(last);
    (first.format("%Y-%m-%d").to_string(), last.format("%Y-%m-%d").to_string())
}

impl MicroParquetApp {
    pub(crate) fn open_mudaraba(&mut self) {
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        if self.mud.start.trim().is_empty() {
            let (s, e) = last_month(chrono::Local::now().date_naive());
            self.mud.start = s;
            self.mud.end = e;
        }
        if !cols.contains(&self.mud.product_col) {
            self.mud.product_col = guess(&cols, &["product", "scheme", "prod_code"]);
        }
        if !cols.contains(&self.mud.balance_col) {
            self.mud.balance_col = guess(&cols, &["avg", "average", "balance", "amount"]);
        }
        self.mud.params = None;
        self.active_tool = ActiveToolModal::Mudaraba;
    }

    fn refresh_mud_params(&mut self) {
        let key = self.mud.end.trim().to_string();
        if matches!(&self.mud.params, Some((k, _)) if *k == key) {
            return;
        }
        let r = match chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d") {
            Ok(d) => load_psr(d).map_err(|e| format!("{:#}", e)),
            Err(_) => Err("Type the end date as YYYY-MM-DD (for example 2026-10-31).".to_string()),
        };
        self.mud.params = Some((key, r));
    }

    /// What the dialog would build, or the first thing that is missing.
    pub(crate) fn mudaraba_source(&self) -> Result<(String, TabSource), String> {
        let m = &self.mud;
        let tab = self.tabs.get(self.active_tab).ok_or_else(|| "Open the deposit tab first.".to_string())?;
        let cols = tab.available_columns();
        for (what, c) in [("product code", &m.product_col), ("balance", &m.balance_col)] {
            if c.trim().is_empty() || !cols.contains(c) {
                return Err(format!("Choose the {} column.", what));
            }
        }
        let income = number(&m.income).filter(|v| *v >= 0.0).ok_or_else(|| "Type the investment income of the period as a number (0 or more).".to_string())?;
        let other = number(&m.other).filter(|v| *v >= 0.0).ok_or_else(|| "Type the other investable funds as a number (0 or more).".to_string())?;
        let spec = MudarabaSpec { product_col: m.product_col.clone(), balance_col: m.balance_col.clone(), daily: m.daily, start: m.start.trim().to_string(), end: m.end.trim().to_string(), income, other_investable: other };
        spec.dates().map_err(|e| format!("{:#}", e))?;
        match &m.params {
            Some((k, Err(e))) if *k == spec.end => return Err(e.clone()),
            _ => {}
        }
        let exec = tab.exec_steps.min(tab.steps.len());
        Ok(("Mudaraba profit distribution".to_string(), TabSource::MudarabaPool { source: Box::new(tab.source.clone()), steps: tab.steps[..exec].to_vec(), spec }))
    }

    pub(crate) fn render_mudaraba_modal(&mut self, ctx: &egui::Context, cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = MudRects::default();
        let mut close = false;
        let mut build = false;
        self.refresh_mud_params();
        let problem = self.mudaraba_source().err();
        let sample = matches!(&self.mud.params, Some((_, Ok(f))) if f.meta.sample);
        let title = format!("{}  Mudaraba profit distribution", theme::ph::SCALES);
        theme::modal(&title).default_width(theme::MIGRATION_DIALOG_W).fill_height().show(ctx, |d| {
            d.header(|ui| {
                if sample {
                    let r = ui.scope(|ui| theme::warning_label(ui, "Sample parameters: the PSR file in use is marked sample. The method, the weightages and the PSR must be those approved by the bank's Shariah Supervisory Committee before the figures are used."));
                    rects.banner = Some(r.response.rect);
                }
            });
            d.body_scroll(|ui| {
                egui::Grid::new("mud_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    let mut pick = |ui: &mut egui::Ui, label: &str, id: &str, cur: &mut String, rects: &mut MudRects| {
                        ui.label(label);
                        let shown = if cur.is_empty() { "Choose\u{2026}".to_string() } else { cur.clone() };
                        let r = egui::ComboBox::from_id_salt(id).width(theme::MIGRATION_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                            for c in &cols {
                                ui.selectable_value(cur, c.clone(), c);
                            }
                        });
                        rects.combos.push((label.trim_end_matches(':').to_string(), r.response.rect));
                        ui.end_row();
                    };
                    pick(ui, "Product code:", "mud_prod", &mut self.mud.product_col, &mut rects);
                    pick(ui, "Balance:", "mud_bal", &mut self.mud.balance_col, &mut rects);
                    ui.label("The balance is:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.mud.daily, false, "the average of the period");
                        ui.radio_value(&mut self.mud.daily, true, "a daily balance (average = sum \u{f7} days)");
                    });
                    ui.end_row();
                    let mut field = |ui: &mut egui::Ui, label: &str, text: &mut String, rects: &mut MudRects| {
                        ui.label(label);
                        let r = ui.add(egui::TextEdit::singleline(text).desired_width(theme::MIGRATION_COMBO_W));
                        rects.fields.push((label.trim_end_matches(':').to_string(), r.rect));
                        ui.end_row();
                    };
                    field(ui, "Period start (YYYY-MM-DD):", &mut self.mud.start, &mut rects);
                    field(ui, "Period end (YYYY-MM-DD):", &mut self.mud.end, &mut rects);
                    field(ui, "Investment income of the period:", &mut self.mud.income, &mut rects);
                    field(ui, "Other investable funds:", &mut self.mud.other, &mut rects);
                });
                ui.add_space(theme::SPACE_S);
                let card = theme::card(ui, "Parameters (from the file; nothing is typed here)", |ui| match &self.mud.params {
                    Some((_, Ok(f))) => {
                        ui.add(egui::Label::new(format!("{} as of {}: CRR {}%, depositors' PSR {}%, {} weightage(s).", f.meta.file, f.meta.as_of.format("%Y-%m-%d"), f.data.crr_pct, f.data.psr_pct, f.data.weightages.len())).wrap());
                        ui.add(egui::Label::new(egui::RichText::new(format!("Source: {}", f.meta.source)).color(tk.muted).size(theme::TEXT_LABEL)).wrap());
                        let list: Vec<String> = f.data.weightages.iter().map(|w| format!("{} {}", w.product, w.weight)).collect();
                        ui.add(egui::Label::new(egui::RichText::new(format!("Weightages: {}", list.join(", "))).color(tk.muted).size(theme::TEXT_LABEL)).wrap());
                    }
                    Some((_, Err(e))) => {
                        theme::error_label(ui, e);
                    }
                    None => {}
                });
                rects.params = Some(card);
                if let Some(p) = &problem {
                    ui.add_space(theme::SPACE_S);
                    theme::error_label(ui, p);
                }
                ui.add_space(theme::SPACE_S);
                ui.add(egui::Label::new(egui::RichText::new(format!("The tab has {} columns: {}. A product with no weightage in the file stops the run and is named.", MUDARABA_COLUMNS.len(), MUDARABA_COLUMNS.join(", "))).color(tk.muted).size(theme::TEXT_LABEL)).wrap());
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Build distribution")).inner;
                    rects.buttons.push(("Build distribution".to_string(), b.rect));
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
        self.mud.rects = rects;
        if build {
            if let Ok((title, source)) = self.mudaraba_source() {
                let fmt = |d| ColumnFormatSpec { fmt_type: MetricFormatType::StandardCommas, decimals: d };
                let formats: HashMap<String, ColumnFormatSpec> = [("Average balance", fmt(2)), ("Weighted balance", fmt(2)), ("Profit", fmt(6)), ("Weightage", fmt(2)), ("Indicative rate % p.a.", ColumnFormatSpec { fmt_type: MetricFormatType::PercentAsIs, decimals: 4 })].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
                self.create_tab_from_source_with(title, source, Vec::new(), formats, None);
                close = true;
            }
        }
        ModalOutcome { close, step: None }
    }
}
