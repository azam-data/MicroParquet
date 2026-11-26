//! Month-on-month variance (Phase 6 P3): Banking ▸ Treasury & ALM ▸ "MoM variance". Choose the two
//! months' tabs, one or two key columns, the balance (and optionally the rate and a count) and the
//! materiality thresholds; the tool makes the variance table and the commentary as two tabs.

use crate::ui::prelude::*;
use micro_parquet::models::MomView;
use micro_parquet::mom::{mom_formats, MomSpec, COMMENTARY_COLUMN};

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct MomRects {
    pub combos: Vec<(String, egui::Rect)>,
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    /// The commentary box of a commentary tab.
    pub text_box: Option<egui::Rect>,
    pub copy: Option<egui::Rect>,
}

pub struct MomState {
    pub prev_tab: usize,
    pub curr_tab: usize,
    pub key1: String,
    /// Empty: one key only.
    pub key2: String,
    pub bal: String,
    pub rate: String,
    pub count: String,
    pub amount_text: String,
    pub pct_text: String,
    pub scale: NumberDisplayFormat,
    /// The spec a saved run came with (its decimals); the dialog's choices replace the rest.
    pub spec: MomSpec,
    pub rects: MomRects,
}

impl Default for MomState {
    fn default() -> Self {
        let d = MomSpec::default();
        Self { prev_tab: 0, curr_tab: 0, key1: String::new(), key2: String::new(), bal: String::new(), rate: String::new(), count: String::new(), amount_text: format!("{}", d.material_amount), pct_text: format!("{}", d.material_pct), scale: d.scale, spec: d, rects: MomRects::default() }
    }
}

fn guess(cols: &[String], words: &[&str]) -> String {
    cols.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned().unwrap_or_default()
}

fn number(t: &str) -> Option<f64> {
    t.trim().replace(',', "").parse::<f64>().ok().filter(|v| v.is_finite())
}

impl MicroParquetApp {
    /// Opens the dialog with the open tab as the current month and the one before it as the previous.
    pub(crate) fn open_mom(&mut self) {
        let n = self.tabs.len();
        let curr = self.active_tab.min(n.saturating_sub(1));
        let prev = if curr > 0 { curr - 1 } else { 1.min(n.saturating_sub(1)) };
        self.mom.prev_tab = prev;
        self.mom.curr_tab = curr;
        self.mom_pick_columns();
        // a rate column is the usual wish: pick one when the months have one
        let common = self.mom_common_columns();
        if self.mom.rate.is_empty() {
            self.mom.rate = guess(&common, &["rate", "roi"]);
        }
        self.active_tool = ActiveToolModal::MomVariance;
    }

    fn mom_common_columns(&self) -> Vec<String> {
        let a = self.tabs.get(self.mom.prev_tab).map(|t| t.available_columns()).unwrap_or_default();
        let b = self.tabs.get(self.mom.curr_tab).map(|t| t.available_columns()).unwrap_or_default();
        a.into_iter().filter(|c| b.contains(c)).collect()
    }

    /// Keeps the columns that both months have; guesses the ones that are empty.
    fn mom_pick_columns(&mut self) {
        let common = self.mom_common_columns();
        let keep = |cur: &str, guess: String, allow_empty: bool| -> String {
            if (cur.is_empty() && allow_empty) || common.iter().any(|c| c == cur) {
                cur.to_string()
            } else {
                guess
            }
        };
        let m = &self.mom;
        let key1 = keep(&m.key1, guess(&common, &["product", "scheme", "segment"]), false);
        let bal = keep(&m.bal, guess(&common, &["balance", "outstanding", "amount"]), false);
        let key2 = keep(&m.key2, String::new(), true);
        let rate = keep(&m.rate, String::new(), true);
        let count = keep(&m.count, String::new(), true);
        self.mom.key1 = key1;
        self.mom.key2 = key2;
        self.mom.bal = bal;
        self.mom.rate = rate;
        self.mom.count = count;
    }

    /// The settings the dialog holds, or the first thing that is missing.
    pub(crate) fn mom_spec(&self) -> Result<MomSpec, String> {
        let m = &self.mom;
        let amount = number(&m.amount_text).filter(|v| *v >= 0.0).ok_or_else(|| "Type the materiality amount as a number (for example 50).".to_string())?;
        let pct = number(&m.pct_text).filter(|v| *v >= 0.0).ok_or_else(|| "Type the materiality percentage as a number (for example 5).".to_string())?;
        let opt = |s: &String| if s.trim().is_empty() { None } else { Some(s.clone()) };
        let mut keys = vec![m.key1.clone()];
        if !m.key2.trim().is_empty() {
            keys.push(m.key2.clone());
        }
        let spec = MomSpec { key_cols: keys, bal_col: m.bal.clone(), rate_col: opt(&m.rate), count_col: opt(&m.count), material_amount: amount, material_pct: pct, scale: m.scale, decimals: m.spec.decimals };
        spec.check().map_err(|e| format!("{:#}", e))?;
        Ok(spec)
    }

    /// The two tabs the dialog would make ("MoM commentary", then "MoM variance"), or why it cannot yet.
    pub(crate) fn mom_sources(&self) -> Result<Vec<(String, TabSource)>, String> {
        if self.tabs.len() < 2 {
            return Err("Open the previous month and the current month in two tabs first.".to_string());
        }
        let (Some(p), Some(c)) = (self.tabs.get(self.mom.prev_tab), self.tabs.get(self.mom.curr_tab)) else {
            return Err("The selected previous or current tab no longer exists. Pick both tabs again.".to_string());
        };
        if self.mom.prev_tab == self.mom.curr_tab {
            return Err("Choose two different tabs: the previous month and the current month.".to_string());
        }
        let spec = self.mom_spec()?;
        let make = |view| TabSource::MomVariance { prev_source: Box::new(p.source.clone()), prev_steps: p.steps.clone(), curr_source: Box::new(c.source.clone()), curr_steps: c.steps.clone(), spec: spec.clone(), view };
        let mut tabs = vec![("MoM commentary".to_string(), make(MomView::Commentary)), ("MoM variance".to_string(), make(MomView::Variance))];
        // Phase 11: the bridge (volume, mix and rate; or the balance bridge without a rate column)
        tabs.push(("MoM bridge".to_string(), make(MomView::Bridge)));
        Ok(tabs)
    }

    pub(crate) fn render_mom_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        let mut build = false;
        let mut rects = MomRects::default();
        self.mom_pick_columns();
        let problem = self.mom_sources().err();
        let title = format!("{}  MoM variance", theme::ph::CHART_LINE);
        theme::modal(&title).default_width(theme::MIGRATION_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                if self.tabs.len() < 2 {
                    ui.label("Open the previous month and the current month in two tabs first.");
                    return;
                }
                let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
                let common = self.mom_common_columns();
                egui::Grid::new("mom_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    let mut pick_tab = |ui: &mut egui::Ui, label: &str, id: &str, cur: &mut usize| {
                        ui.label(label);
                        let r = egui::ComboBox::from_id_salt(id).width(theme::MIGRATION_COMBO_W).selected_text(titles.get(*cur).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                            for (i, t) in titles.iter().enumerate() {
                                ui.selectable_value(cur, i, t);
                            }
                        });
                        rects.combos.push((label.to_string(), r.response.rect));
                        ui.end_row();
                    };
                    pick_tab(ui, "Previous-month tab:", "mom_prev", &mut self.mom.prev_tab);
                    pick_tab(ui, "Current-month tab:", "mom_curr", &mut self.mom.curr_tab);
                    let mut pick_col = |ui: &mut egui::Ui, label: &str, id: &str, cur: &mut String, optional: bool| {
                        ui.label(label);
                        let shown = if cur.is_empty() { if optional { "(none)".to_string() } else { "Choose\u{2026}".to_string() } } else { cur.clone() };
                        let r = egui::ComboBox::from_id_salt(id).width(theme::MIGRATION_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                            if optional {
                                ui.selectable_value(cur, String::new(), "(none)");
                            }
                            for c in &common {
                                ui.selectable_value(cur, c.clone(), c);
                            }
                        });
                        rects.combos.push((label.to_string(), r.response.rect));
                        ui.end_row();
                    };
                    pick_col(ui, "Key:", "mom_key1", &mut self.mom.key1, false);
                    pick_col(ui, "Second key (optional):", "mom_key2", &mut self.mom.key2, true);
                    pick_col(ui, "Balance:", "mom_bal", &mut self.mom.bal, false);
                    pick_col(ui, "Rate % (optional):", "mom_rate", &mut self.mom.rate, true);
                    pick_col(ui, "Account count (optional):", "mom_cnt", &mut self.mom.count, true);
                    ui.label("Material when the balance moves by:");
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut self.mom.amount_text).desired_width(theme::CF_DEFAULT_W));
                        rects.fields.push(("Amount".to_string(), r.rect));
                        let cur = self.mom.scale;
                        let r = egui::ComboBox::from_id_salt("mom_scale").selected_text(cur.label()).show_ui(ui, |ui| {
                            for f in NumberDisplayFormat::all() {
                                ui.selectable_value(&mut self.mom.scale, *f, f.label());
                            }
                        });
                        rects.combos.push(("Scale".to_string(), r.response.rect));
                    });
                    ui.end_row();
                    ui.label("or by this share of last month:");
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut self.mom.pct_text).desired_width(theme::CF_DEFAULT_W));
                        rects.fields.push(("Percent".to_string(), r.rect));
                        ui.label("%");
                    });
                    ui.end_row();
                });
                if let Some(p) = &problem {
                    ui.add_space(theme::SPACE_S);
                    theme::error_label(ui, p);
                }
                ui.add_space(theme::SPACE_S);
                ui.add(egui::Label::new(egui::RichText::new("The tool makes three tabs: the variance table (balance, rate and annual interest of each month, the interest change split into volume, rate and mix effects) and the commentary (one sentence per material key, from fixed templates) and the bridge (the interest change as volume, mix and rate that add up exactly, or the change of balance by key; draw it with Chart this). The thresholds are saved with the tabs.").color(tk.muted).size(theme::TEXT_LABEL)).wrap());
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.tabs.len() >= 2 {
                        let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Build MoM variance")).inner;
                        rects.buttons.push(("Build MoM variance".to_string(), b.rect));
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
        self.mom.rects.combos = rects.combos;
        self.mom.rects.fields = rects.fields;
        self.mom.rects.buttons = rects.buttons;
        if build {
            if let Ok(sources) = self.mom_sources() {
                if let Ok(spec) = self.mom_spec() {
                    let formats = mom_formats(&spec);
                    for (title, source) in sources {
                        self.create_tab_from_source_with(title, source, Vec::new(), formats.clone(), None);
                    }
                    close = true;
                }
            }
        }
        ModalOutcome { close, step: None }
    }

    #[cfg(test)]
    pub(crate) fn mom_commentary_text_for_test(&self) -> Option<(String, usize, usize)> {
        self.mom_commentary_text()
    }

    /// The commentary of the active tab as one text, for the box above its table.
    fn mom_commentary_text(&self) -> Option<(String, usize, usize)> {
        let tab = self.tabs.get(self.active_tab)?;
        if !matches!(&tab.source, TabSource::MomVariance { view: MomView::Commentary, .. }) {
            return None;
        }
        let col = tab.cached_slice.column(COMMENTARY_COLUMN).ok()?.str().ok()?;
        let lines: Vec<String> = col.into_iter().flatten().map(str::to_string).collect();
        Some((lines.join("\n"), lines.len(), tab.total_rows))
    }

    /// The commentary tab's text box: the lines in one selectable text and a Copy button.
    pub(crate) fn render_mom_commentary(&mut self, ui: &mut egui::Ui) {
        let Some((text, shown, total)) = self.mom_commentary_text() else {
            return;
        };
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = MomRects::default();
        egui::Frame::none().stroke(theme::line(tk.border)).rounding(theme::RADIUS_CONTROL).inner_margin(theme::SPACE_S).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(theme::semibold("Commentary"));
                let b = ui.button("Copy");
                rects.copy = Some(b.rect);
                if b.on_hover_text("Copy the commentary to the clipboard").clicked() {
                    ui.ctx().copy_text(text.clone());
                    self.status_message = "Copied the commentary.".to_string();
                }
                if total > shown {
                    ui.label(egui::RichText::new(format!("Showing {} of {} lines; the table below has the rest.", shown, total)).color(tk.muted).size(theme::TEXT_LABEL));
                }
            });
            let mut shown_text = text.as_str();
            let r = ui.add(egui::TextEdit::multiline(&mut shown_text).desired_width(f32::INFINITY).desired_rows(theme::MOM_BOX_ROWS.min(shown.max(1))));
            rects.text_box = Some(r.rect);
        });
        self.mom.rects.text_box = rects.text_box;
        self.mom.rects.copy = rects.copy;
    }
}
