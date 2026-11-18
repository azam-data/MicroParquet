//! The behaviour profile dialog (Phase 5 R2): core and volatile parts of non-maturity deposits from
//! a balance history, the prepayment and early-redemption rates of loans and term deposits, the
//! Bangladesh Bank caps (BRPD IRRBB guideline 2021, Table 4), and "Save profile" to
//! `portable_data/behaviour/<date>.json`.

use crate::ui::prelude::*;
use micro_parquet::behaviour::{build_product, history_from_lazy, save_behaviour, series_by_product, short_history_warning, BehaviourFile, CoreMethod, RowInput, RowOutcome, DEFAULT_Z};
use micro_parquet::rules::{load_nmd_caps, NmdCapsFile, RuleFile};

/// One product line of the dialog. Texts are what the user typed.
#[derive(Clone, Debug, Default)]
pub struct ProductLine {
    pub product: String,
    /// The product came from the history (it cannot be renamed).
    pub from_history: bool,
    pub category: String,
    pub avg_maturity: String,
    pub method: CoreMethod,
    pub typed_core: String,
    pub cpr: String,
    pub tdrr: String,
}

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct BehaviourRects {
    pub header: Vec<(String, egui::Rect)>,
    pub table: egui::Rect,
    pub row_controls: Vec<(usize, String, egui::Rect)>,
    pub messages: egui::Rect,
    pub buttons: Vec<(String, egui::Rect)>,
}

impl Default for BehaviourRects {
    fn default() -> Self {
        Self { header: Vec::new(), table: egui::Rect::NOTHING, row_controls: Vec::new(), messages: egui::Rect::NOTHING, buttons: Vec::new() }
    }
}

pub struct BehaviourState {
    pub tab: usize,
    pub date_col: String,
    pub product_col: String,
    pub balance_col: String,
    pub lookback: String,
    pub z: String,
    pub as_of: String,
    pub source: String,
    pub rows: Vec<ProductLine>,
    /// The month-end series of each product of the history that was read.
    pub series: Vec<(String, Vec<(chrono::NaiveDate, f64)>)>,
    pub read_note: String,
    pub read_error: Option<String>,
    /// The caps rule file for the date the profile is for.
    pub caps: Option<(String, Result<RuleFile<NmdCapsFile>, String>)>,
    pub saved_note: Option<String>,
    pub rects: BehaviourRects,
}

impl Default for BehaviourState {
    fn default() -> Self {
        Self {
            tab: 0,
            date_col: String::new(),
            product_col: String::new(),
            balance_col: String::new(),
            lookback: "120".to_string(),
            z: format!("{}", DEFAULT_Z),
            as_of: String::new(),
            source: String::new(),
            rows: Vec::new(),
            series: Vec::new(),
            read_note: String::new(),
            read_error: None,
            caps: None,
            saved_note: None,
            rects: BehaviourRects::default(),
        }
    }
}

fn guess(columns: &[String], words: &[&str]) -> String {
    columns.iter().find(|c| words.iter().any(|w| c.to_lowercase().contains(w))).cloned().unwrap_or_default()
}

fn parse_num(t: &str) -> Result<Option<f64>, String> {
    let t = t.trim().trim_end_matches('%').trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.replace(',', "").parse::<f64>().map(Some).map_err(|_| format!("'{}' is not a number.", t))
}

/// The result of one line: the profile product or what is wrong.
pub type LineResult = Result<RowOutcome, String>;

impl BehaviourState {
    fn series_of(&self, product: &str) -> Option<&[(chrono::NaiveDate, f64)]> {
        let want = micro_parquet::behaviour::normalise_product(product);
        self.series.iter().find(|(p, _)| *p == want).map(|(_, s)| s.as_slice())
    }

    /// The caps file for the profile's date (today when the date is not typed yet).
    fn refresh_caps(&mut self) {
        let on = chrono::NaiveDate::parse_from_str(self.as_of.trim(), "%Y-%m-%d").unwrap_or_else(|_| chrono::Local::now().date_naive());
        let key = on.format("%Y-%m-%d").to_string();
        if matches!(&self.caps, Some((k, _)) if *k == key) {
            return;
        }
        self.caps = Some((key, load_nmd_caps(on).map_err(|e| format!("{:#}", e))));
    }

    /// The result of every line, or the problem that stops all of them (settings).
    pub fn results(&self) -> Result<Vec<LineResult>, String> {
        let lookback: u32 = self.lookback.trim().parse().map_err(|_| "Type the look-back as a whole number of months (the guideline uses 120).".to_string())?;
        if lookback == 0 {
            return Err("The look-back must be at least 1 month.".to_string());
        }
        let z: f64 = self.z.trim().parse().map_err(|_| "Type z as a number (2.33 is the 99% value).".to_string())?;
        let caps = match &self.caps {
            Some((_, Ok(c))) => c,
            Some((_, Err(e))) => return Err(e.clone()),
            None => return Err("The caps file has not been read yet.".to_string()),
        };
        Ok(self
            .rows
            .iter()
            .map(|l| {
                let name = l.product.trim();
                let num = |t: &str, what: &str| parse_num(t).map_err(|e| format!("'{}': {} {}", name, what, e));
                let input = RowInput {
                    product: l.product.clone(),
                    category: l.category.clone(),
                    avg_maturity_years: num(&l.avg_maturity, "average maturity")?,
                    method: l.method,
                    typed_core_pct: num(&l.typed_core, "core share")?,
                    cpr_pct: num(&l.cpr, "CPR")?.unwrap_or(0.0),
                    tdrr_pct: num(&l.tdrr, "TDRR")?.unwrap_or(0.0),
                };
                let series = self.series_of(&l.product);
                build_product(&input, series, lookback, z, caps).map_err(|e| format!("{:#}", e))
            })
            .collect())
    }

    /// The profile that would be saved, or the first thing that is wrong.
    pub fn profile(&self) -> Result<BehaviourFile, String> {
        let results = self.results()?;
        if results.is_empty() {
            return Err("Add at least one product (read a history, or Add product).".to_string());
        }
        let mut products = Vec::new();
        for r in results {
            products.push(r?.product);
        }
        Ok(BehaviourFile { as_of: self.as_of.trim().to_string(), source: self.source.trim().to_string(), products })
    }

    /// The shortest history (in months) any product's core share is built on; `None` when none uses one.
    pub fn shortest_history(results: &[LineResult]) -> Option<u32> {
        results.iter().filter_map(|r| r.as_ref().ok()).map(|o| o.months_used).filter(|m| *m > 0).min()
    }
}

impl MicroParquetApp {
    pub(crate) fn open_behaviour_profile(&mut self) {
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let b = &mut self.behaviour;
        if !self.tabs.is_empty() {
            b.tab = self.active_tab;
            if b.date_col.is_empty() || !cols.contains(&b.date_col) {
                b.date_col = guess(&cols, &["date", "dt", "period", "month"]);
            }
            if b.product_col.is_empty() || !cols.contains(&b.product_col) {
                b.product_col = guess(&cols, &["product", "scheme", "prod_code"]);
            }
            if b.balance_col.is_empty() || !cols.contains(&b.balance_col) {
                b.balance_col = guess(&cols, &["balance", "amount", "outstanding"]);
            }
        }
        b.caps = None;
        b.saved_note = None;
        self.active_tool = ActiveToolModal::BehaviourProfile;
    }

    /// Reads the history of the chosen tab: the month-end balance of each product, and a line for
    /// each product that has no line yet.
    pub(crate) fn read_behaviour_history(&mut self) {
        self.behaviour.read_error = None;
        let Some(tab) = self.tabs.get(self.behaviour.tab) else {
            self.behaviour.read_error = Some("Open the tab with the balance history first.".to_string());
            return;
        };
        let (title, source, steps) = (tab.title.clone(), tab.source.clone(), tab.steps.clone());
        let (d, p, b) = (self.behaviour.date_col.clone(), self.behaviour.product_col.clone(), self.behaviour.balance_col.clone());
        for (c, what) in [(&d, "date"), (&p, "product"), (&b, "balance")] {
            if c.trim().is_empty() {
                self.behaviour.read_error = Some(format!("Choose the {} column of the history.", what));
                return;
            }
        }
        let result = micro_parquet::engine::compile_tab_lazy(&source, &steps, &self.paths.temp_dir).and_then(|lf| history_from_lazy(lf, &d, &p, &b));
        match result {
            Ok(rows) => {
                let series = series_by_product(&rows);
                let last = series.iter().filter_map(|(_, s)| s.last().map(|x| x.0)).max();
                let first = series.iter().filter_map(|(_, s)| s.first().map(|x| x.0)).min();
                self.behaviour.read_note = format!(
                    "Read {} product(s), month-ends {} to {}.",
                    series.len(),
                    first.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
                    last.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
                );
                for (product, _) in &series {
                    match self.behaviour.rows.iter_mut().find(|l| micro_parquet::behaviour::normalise_product(&l.product) == *product) {
                        Some(l) => l.from_history = true,
                        None => self.behaviour.rows.push(ProductLine { product: product.clone(), from_history: true, ..ProductLine::default() }),
                    }
                }
                if self.behaviour.as_of.trim().is_empty() {
                    if let Some(l) = last {
                        self.behaviour.as_of = l.format("%Y-%m-%d").to_string();
                    }
                }
                if self.behaviour.source.trim().is_empty() {
                    self.behaviour.source = format!("MicroParquet behaviour profile from the balance history in '{}'", title);
                }
                self.behaviour.series = series;
                self.behaviour.caps = None;
            }
            Err(e) => {
                self.behaviour.series.clear();
                self.behaviour.read_error = Some(format!("{:#}", e));
            }
        }
    }

    pub(crate) fn render_behaviour_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut save = false;
        let mut read = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  Behaviour profile", theme::ph::CALCULATOR);
        let mut rects = BehaviourRects::default();
        self.behaviour.refresh_caps();
        let results = self.behaviour.results();
        let profile = self.behaviour.profile();
        let dir = micro_parquet::rules::behaviour_dir();
        let exists = chrono::NaiveDate::parse_from_str(self.behaviour.as_of.trim(), "%Y-%m-%d").is_ok_and(|d| dir.join(format!("{}.json", d.format("%Y-%m-%d"))).exists());
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
        let columns: Vec<Vec<String>> = self.tabs.iter().map(|t| t.available_columns()).collect();
        let categories: Vec<String> = match &self.behaviour.caps {
            Some((_, Ok(c))) => c.data.categories.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        };
        theme::modal(&title).default_width(theme::BEHAVIOUR_DIALOG_W).fill_height().show(ctx, |d| {
            d.header(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let cols = columns.get(self.behaviour.tab).cloned().unwrap_or_default();
                    ui.label(egui::RichText::new("History tab").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = egui::ComboBox::from_id_salt("bh_tab").width(theme::GAP_COMBO_W).selected_text(titles.get(self.behaviour.tab).cloned().unwrap_or_else(|| "(no tab)".to_string())).show_ui(ui, |ui| {
                        for (t, name) in titles.iter().enumerate() {
                            ui.selectable_value(&mut self.behaviour.tab, t, name);
                        }
                    });
                    rects.header.push(("History tab".to_string(), r.response.rect));
                    for (label, field) in [("Date", 0), ("Product", 1), ("Balance", 2)] {
                        ui.label(egui::RichText::new(label).color(tk.muted).size(theme::TEXT_LABEL));
                        let value = match field {
                            0 => &mut self.behaviour.date_col,
                            1 => &mut self.behaviour.product_col,
                            _ => &mut self.behaviour.balance_col,
                        };
                        let shown = if value.is_empty() { "Choose\u{2026}".to_string() } else { value.clone() };
                        let r = egui::ComboBox::from_id_salt(("bh_col", field)).width(theme::GAP_COMBO_NARROW_W).selected_text(shown).show_ui(ui, |ui| {
                            for c in &cols {
                                ui.selectable_value(value, c.clone(), c.as_str());
                            }
                        });
                        rects.header.push((label.to_string(), r.response.rect));
                    }
                    let b = ui.add_enabled(!self.tabs.is_empty(), egui::Button::new("Read history"));
                    rects.buttons.push(("Read history".to_string(), b.rect));
                    if b.clicked() {
                        read = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Look-back (months)").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.behaviour.lookback).desired_width(theme::BEHAVIOUR_CELL_W));
                    rects.header.push(("Look-back".to_string(), r.rect));
                    ui.label(egui::RichText::new("z").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.behaviour.z).desired_width(theme::BEHAVIOUR_CELL_W));
                    rects.header.push(("z".to_string(), r.rect));
                    ui.label(egui::RichText::new("Profile date (YYYY-MM-DD)").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.behaviour.as_of).desired_width(theme::CF_DEFAULT_W));
                    rects.header.push(("Profile date".to_string(), r.rect));
                    ui.label(egui::RichText::new("Source").color(tk.muted).size(theme::TEXT_LABEL));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.behaviour.source).desired_width(theme::BEHAVIOUR_SOURCE_W));
                    rects.header.push(("Source".to_string(), r.rect));
                });
            });
            d.body_scroll(|ui| {
                if !self.behaviour.read_note.is_empty() {
                    ui.label(egui::RichText::new(self.behaviour.read_note.clone()).color(tk.muted).size(theme::TEXT_LABEL));
                }
                let grid = egui::Grid::new("bh_rows").num_columns(10).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                    for h in ["Product", "Category", "Average maturity (years)", "Method", "Typed core %", "Core %", "Months", "CPR %", "TDRR %", ""] {
                        ui.label(egui::RichText::new(h).color(tk.muted).size(theme::TEXT_LABEL));
                    }
                    ui.end_row();
                    let mut remove: Option<usize> = None;
                    for i in 0..self.behaviour.rows.len() {
                        let mut line = self.behaviour.rows[i].clone();
                        let mut push = |name: &str, r: egui::Rect| rects.row_controls.push((i, name.to_string(), r));
                        if line.from_history {
                            let r = ui.label(line.product.clone());
                            push("Product", r.rect);
                        } else {
                            let r = ui.add(egui::TextEdit::singleline(&mut line.product).desired_width(theme::BEHAVIOUR_CELL_W));
                            push("Product", r.rect);
                        }
                        let shown = if line.category.is_empty() { "(none)".to_string() } else { line.category.clone() };
                        let r = egui::ComboBox::from_id_salt(("bh_cat", i)).width(theme::CF_COMBO_W).selected_text(shown).show_ui(ui, |ui| {
                            ui.selectable_value(&mut line.category, String::new(), "(none)");
                            for c in &categories {
                                ui.selectable_value(&mut line.category, c.clone(), c.as_str());
                            }
                        });
                        push("Category", r.response.rect);
                        let r = ui.add(egui::TextEdit::singleline(&mut line.avg_maturity).desired_width(theme::BEHAVIOUR_CELL_W));
                        push("Average maturity", r.rect);
                        let r = egui::ComboBox::from_id_salt(("bh_method", i)).width(theme::CF_DEFAULT_W).selected_text(line.method.label()).show_ui(ui, |ui| {
                            for m in CoreMethod::ALL {
                                ui.selectable_value(&mut line.method, m, m.label());
                            }
                        });
                        push("Method", r.response.rect);
                        let r = ui.add_enabled(line.method == CoreMethod::Typed, egui::TextEdit::singleline(&mut line.typed_core).desired_width(theme::BEHAVIOUR_CELL_W));
                        push("Typed core %", r.rect);
                        let (core, months) = match results.as_ref().ok().and_then(|r| r.get(i)) {
                            Some(Ok(o)) if !o.product.category.is_empty() => (format!("{:.2}{}", o.product.core_pct, if o.product.capped { " capped" } else { "" }), if o.months_used > 0 { o.months_used.to_string() } else { "\u{2014}".to_string() }),
                            _ => ("\u{2014}".to_string(), "\u{2014}".to_string()),
                        };
                        let r = ui.label(core);
                        push("Core %", r.rect);
                        let r = ui.label(months);
                        push("Months", r.rect);
                        let r = ui.add(egui::TextEdit::singleline(&mut line.cpr).desired_width(theme::BEHAVIOUR_CELL_W));
                        push("CPR %", r.rect);
                        let r = ui.add(egui::TextEdit::singleline(&mut line.tdrr).desired_width(theme::BEHAVIOUR_CELL_W));
                        push("TDRR %", r.rect);
                        let x = ui.add(egui::Button::new(egui::RichText::new(theme::ph::X).color(tk.muted)).frame(false)).on_hover_text("Remove this product");
                        push("Remove", x.rect);
                        if x.clicked() {
                            remove = Some(i);
                        }
                        self.behaviour.rows[i] = line;
                        ui.end_row();
                    }
                    if let Some(i) = remove {
                        self.behaviour.rows.remove(i);
                    }
                });
                rects.table = grid.response.rect;
                if ui.button(format!("{}  Add product", theme::ph::PLUS)).clicked() {
                    self.behaviour.rows.push(ProductLine::default());
                }
                ui.add_space(theme::SPACE_S);
                let msgs = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: String| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    if let Some(e) = &self.behaviour.read_error {
                        theme::error_label(ui, e);
                    }
                    match &results {
                        Err(e) => theme::error_label(ui, e),
                        Ok(rs) => {
                            for (l, r) in self.behaviour.rows.iter().zip(rs) {
                                if let Err(e) = r {
                                    theme::error_label(ui, e);
                                } else if l.product.trim().is_empty() {
                                    theme::error_label(ui, "A product has no code.");
                                }
                            }
                            if let Some(m) = BehaviourState::shortest_history(rs) {
                                if let Some(w) = short_history_warning(m) {
                                    theme::warning_label(ui, &w);
                                }
                            }
                        }
                    }
                    if let Some(n) = &self.behaviour.saved_note {
                        ui.colored_label(tk.success, n.clone());
                    }
                    if let Some((_, Ok(c))) = &self.behaviour.caps {
                        ui.add(muted(format!("Caps: {} ({}).", c.meta.file, c.meta.source)));
                    }
                    ui.add(muted("Historical minimum: core % = lowest month-end balance in the look-back \u{f7} latest balance \u{d7} 100. Volatility: core % = (1 \u{2212} z \u{d7} \u{3c3}) \u{d7} 100, with \u{3c3} the sample standard deviation of the month-on-month changes. A computed core above the cap of its category is cut to it (\"capped\"); a typed core or average maturity above the cap is refused. CPR (loan prepayment) and TDRR (term-deposit early redemption) are typed by hand; leave a product's category at (none) if it only has them. A history with several rows per product and date is summed; daily history is reduced to month-end.".to_string()));
                });
                rects.messages = msgs.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let label = if exists { "Replace profile" } else { "Save profile" };
                    let b = ui.add_enabled_ui(profile.is_ok(), |ui| theme::primary_button(ui, label)).inner;
                    rects.buttons.push((label.to_string(), b.rect));
                    if b.clicked() {
                        save = true;
                    }
                    if let Err(e) = &profile {
                        ui.label(egui::RichText::new(e.clone()).color(tk.muted).size(theme::TEXT_LABEL));
                    }
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.behaviour.rects = rects;
        if read {
            self.read_behaviour_history();
        }
        if save {
            match self.behaviour.profile().map_err(|e| anyhow::anyhow!(e)).and_then(|f| save_behaviour(&dir, &f)) {
                Ok(path) => {
                    let n = format!("Saved {}.", path.display());
                    self.status_message = n.clone();
                    self.behaviour.saved_note = Some(n);
                }
                Err(e) => {
                    self.behaviour.saved_note = None;
                    self.status_message = format!("The profile was not saved: {:#}", e);
                }
            }
        }
        ModalOutcome { close, step: None }
    }
}
