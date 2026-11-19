//! Screen module `chart`: View ▸ Chart (Phase 6 U5). A category, an optional series, a value and an
//! aggregation, calculated on the worker over the whole tab; bars or a line with the category
//! names on the x-axis; "Open chart data as tab".

use crate::ui::prelude::*;
use micro_parquet::chart::{chart_table, ChartAgg, ChartData, ChartKind, ChartScale, ChartSpec, DEFAULT_TOP_N};
use micro_parquet::models::MoneyScale;

/// What the drawer drew (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct ChartRects {
    pub controls: Option<egui::Rect>,
    pub fields: Vec<(String, egui::Rect)>,
    pub title: Option<egui::Rect>,
    pub plot: Option<egui::Rect>,
    pub notes: Option<egui::Rect>,
    /// The "Pop out" (or "Dock back") button of the side panel's Chart tab.
    pub pop_button: Option<egui::Rect>,
}

/// State of this screen.
pub struct ChartState {
    pub spec: ChartSpec,
    /// The tab the settings belong to, and its key in `settings.json`.
    pub tab_id: Option<u64>,
    pub tab_key: String,
    pub data: Option<ChartData>,
    pub error: Option<String>,
    pub busy: bool,
    /// The last request sent (a counter), and what it was for (settings and pipeline).
    pub token: u64,
    pub submitted: Option<String>,
    /// The settings as last written to `settings.json`.
    pub saved: String,
    pub rects: ChartRects,
    /// The Table view instead of the plot.
    pub show_table: bool,
}

/// A chart that is a tab of its own ("Open as tab"): what it shows and in what scale.
#[derive(Clone, Debug)]
pub struct ChartTabInfo {
    pub data: ChartData,
    pub show_table: bool,
    pub divisor: f64,
    pub decimals: usize,
    pub unit: String,
}

impl Default for ChartState {
    fn default() -> Self {
        Self { spec: ChartSpec::default(), tab_id: None, tab_key: String::new(), data: None, error: None, busy: false, token: 0, submitted: None, saved: String::new(), rects: ChartRects::default(), show_table: false }
    }
}

/// The word for a scale in a title: "Crore".
pub fn scale_word(s: &MoneyScale) -> &'static str {
    match s.suffix {
        "Cr" => "Crore",
        "L" => "Lakh",
        "M" => "Million",
        "K" => "Thousand",
        _ => "",
    }
}

/// A category label cut to `max` characters with "…" (the whole label is in the tooltip).
pub fn short_label(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        label.to_string()
    } else {
        format!("{}\u{2026}", label.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

/// The key a tab's chart choices are kept under: its file name, else its title.
fn tab_key(tab: &TabState) -> String {
    micro_parquet::steps_edit::source_files(&tab.source).first().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| tab.title.clone())
}

fn is_number_type(dt: &polars::prelude::DataType) -> bool {
    dt.is_numeric() || matches!(dt, polars::prelude::DataType::Decimal(..))
}

impl MicroParquetApp {
    /// The scale the chart shows its values in (`None` = as they are), from the value column's
    /// display format or the chart's own choice.
    pub(crate) fn chart_scale(&self, spec: &ChartSpec) -> Option<MoneyScale> {
        if spec.agg == ChartAgg::Count {
            return None;
        }
        match spec.scale.fixed() {
            Some(s) => s,
            None => {
                let col = spec.value.as_deref()?;
                let fmt = self.tabs.get(self.active_tab).and_then(|t| t.col_formats.get(col));
                micro_parquet::models::resolved_scale(col, fmt, self.num_format)
            }
        }
    }

    /// One value as the tooltip writes it: in the chart's scale, or in the column's own format.
    pub(crate) fn chart_value_text(&self, spec: &ChartSpec, v: f64) -> String {
        if spec.agg == ChartAgg::Count {
            return format_with_commas(v, 0);
        }
        if let Some(s) = spec.scale.fixed() {
            return match s {
                Some(sc) => sc.format(v, self.global_decimals),
                None => format_with_commas(v, self.global_decimals),
            };
        }
        let col = spec.value.as_deref().unwrap_or("");
        let fmt = self.tabs.get(self.active_tab).and_then(|t| t.col_formats.get(col));
        format_metric_value(v, col, fmt, self.num_format, self.global_decimals)
    }

    /// Takes the tab's saved choices (or sensible first ones) when the chart meets a tab.
    fn chart_attach_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if self.chart.tab_id == Some(tab.id) {
            return;
        }
        let key = tab_key(tab);
        let saved = self.settings.chart.get(&key).and_then(|v| serde_json::from_value::<ChartSpec>(v.clone()).ok());
        let cols = tab.available_columns();
        let spec = match saved {
            // a saved choice that names a column this tab no longer has is not used
            Some(s) if cols.contains(&s.category) && s.value.as_ref().is_none_or(|v| cols.contains(v)) && s.series.as_ref().is_none_or(|v| cols.contains(v)) => s,
            _ => {
                let numeric: Vec<String> = tab.cached_slice.get_columns().iter().filter(|c| is_number_type(c.dtype())).map(|c| c.name().to_string()).collect();
                let category = tab.cached_slice.get_columns().iter().find(|c| !is_number_type(c.dtype())).map(|c| c.name().to_string()).or_else(|| cols.first().cloned()).unwrap_or_default();
                let value = micro_parquet::tab_jobs::guess_balance_column(&tab.cached_slice).filter(|b| numeric.contains(b)).or_else(|| numeric.first().cloned());
                ChartSpec { category, agg: if value.is_some() { ChartAgg::Sum } else { ChartAgg::Count }, value, top_n: DEFAULT_TOP_N, ..Default::default() }
            }
        };
        self.chart.saved = serde_json::to_string(&spec).unwrap_or_default();
        self.chart = ChartState { spec, tab_id: Some(tab.id), tab_key: key, saved: self.chart.saved.clone(), ..Default::default() };
        self.sort_lists.reload();
    }

    /// Sends the settings to the worker when they (or the data) changed since the last request.
    fn chart_tick(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let exec = tab.exec_steps.min(tab.steps.len());
        let key = format!("{}|{}|{}", tab.id, self.chart.spec.calc_key(), micro_parquet::steps_edit::pipeline_hash(&tab.source, &tab.steps[..exec]));
        if self.chart.submitted.as_deref() == Some(key.as_str()) {
            return;
        }
        self.chart.submitted = Some(key);
        self.chart.token += 1;
        self.chart.busy = true;
        let req = ChartRequest { tab_id: tab.id, token: self.chart.token, source: tab.source.clone(), steps: tab.steps[..exec].to_vec(), spec: self.chart.spec.clone(), temp_dir: self.paths.temp_dir.clone() };
        let jkey = JobKey::tab(JobKind::Chart, tab.id);
        self.jobs.submit(jkey, "Calculating the chart", false, move |ctx| micro_parquet::tab_jobs::compute_chart_job(req, ctx).map(|r| JobOutput::Chart(Box::new(r))));
    }

    /// The worker's answer: it is kept when it is for the latest request.
    pub(crate) fn apply_chart_result(&mut self, r: micro_parquet::tab_jobs::ChartResult) {
        if self.chart.tab_id != Some(r.tab_id) || r.token != self.chart.token {
            return;
        }
        self.chart.busy = false;
        match r.data {
            Ok(d) => {
                self.chart.data = Some(d);
                self.chart.error = None;
            }
            Err(e) => {
                self.chart.data = None;
                self.chart.error = Some(e);
            }
        }
    }

    /// Writes the choices into `settings.json` when they changed.
    pub(crate) fn chart_save_choices(&mut self) {
        let json = serde_json::to_string(&self.chart.spec).unwrap_or_default();
        if json == self.chart.saved || self.chart.tab_key.is_empty() {
            return;
        }
        self.chart.saved = json;
        if let Ok(v) = serde_json::to_value(&self.chart.spec) {
            self.settings.chart.insert(self.chart.tab_key.clone(), v);
            self.save_settings();
        }
    }

    #[cfg(test)]
    pub(crate) fn chart_attach_tab_for_test(&mut self) {
        self.chart_attach_tab();
    }

    #[cfg(test)]
    pub(crate) fn chart_save_choices_for_test(&mut self) {
        self.chart_save_choices();
    }

    /// "Open chart data as tab": the table behind the chart, in a new tab that can be exported.
    pub(crate) fn chart_open_as_tab(&mut self) {
        let Some(data) = self.chart.data.clone() else {
            self.status_message = "There is no chart data yet.".to_string();
            return;
        };
        let result = (|| -> anyhow::Result<TabSource> {
            let mut df = chart_table(&data)?;
            std::fs::create_dir_all(&self.paths.temp_dir)?;
            let path = self.paths.temp_dir.join(format!("chart_data_{}_{}.parquet", std::process::id(), self.chart.token));
            let f = std::fs::File::create(&path)?;
            polars::prelude::ParquetWriter::new(f).finish(&mut df)?;
            Ok(TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() })
        })();
        match result {
            Ok(src) => {
                let what = data.title(None);
                let title = format!("Chart: {}", what.split(':').next().unwrap_or("data"));
                self.create_tab_from_source(title, src, Vec::new(), HashMap::new());
            }
            Err(e) => self.status_message = format!("The chart data could not be opened as a tab: {:#}", e),
        }
    }

    /// "Open as tab": the chart in a tab of its own that fills the main area. The tab is the chart's data
    /// table (so it can be exported like any tab); the main area draws the chart, with a Table toggle.
    pub(crate) fn chart_open_as_chart_tab(&mut self) {
        let Some(data) = self.chart.data.clone() else {
            self.status_message = "There is no chart data yet.".to_string();
            return;
        };
        let scale = self.chart_scale(&self.chart.spec);
        let info = ChartTabInfo {
            data: data.clone(),
            show_table: false,
            divisor: scale.map(|s| s.divisor).unwrap_or(1.0),
            decimals: self.global_decimals,
            unit: scale.as_ref().map(scale_word).unwrap_or("").to_string(),
        };
        let result = (|| -> anyhow::Result<TabSource> {
            let mut df = chart_table(&data)?;
            std::fs::create_dir_all(&self.paths.temp_dir)?;
            let path = self.paths.temp_dir.join(format!("chart_tab_{}_{}.parquet", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
            let f = std::fs::File::create(&path)?;
            polars::prelude::ParquetWriter::new(f).finish(&mut df)?;
            Ok(TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() })
        })();
        match result {
            Ok(src) => {
                let title = format!("Chart: {}", micro_parquet::charts::layout::chart_title(&data));
                let id = self.create_tab_from_source_with(title.clone(), src, Vec::new(), HashMap::new(), None);
                self.chart_tabs.insert(id, info);
                // kept with the session: the chart's own settings, by the title of the tab it charts
                if let Ok(spec) = serde_json::to_value(&data.spec) {
                    self.settings.chart_tabs.push(serde_json::json!({"title": title, "source": self.chart.tab_key, "spec": spec}));
                    self.save_settings();
                }
            }
            Err(e) => self.status_message = format!("The chart could not be opened as a tab: {:#}", e),
        }
    }

    pub(crate) fn active_is_chart_tab(&self) -> bool {
        self.tabs.get(self.active_tab).is_some_and(|t| self.chart_tabs.contains_key(&t.id))
    }

    /// What fills the main area while a chart tab is active: a Table / Chart toggle, then the chart.
    pub(crate) fn render_chart_main(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.tabs.get(self.active_tab).map(|t| t.id) else { return };
        let dark = self.dark_mode;
        let Some(info) = self.chart_tabs.get_mut(&id) else { return };
        let label = if info.show_table { format!("{}  Show chart", egui_phosphor::regular::CHART_BAR) } else { format!("{}  Table", egui_phosphor::regular::TABLE) };
        let b = ui.button(label);
        if b.clicked() {
            info.show_table = !info.show_table;
        }
        ui.add_space(theme::SPACE_S);
        let opts = micro_parquet::charts::layout::LayoutOpts { dark, divisor: info.divisor, decimals: info.decimals, unit: info.unit.clone(), bare: false, colours: None };
        if info.show_table {
            crate::ui::chart_render::show_table(ui, &info.data, &opts);
        } else {
            let data = info.data.clone();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let h = ui.available_height().max(theme::CHART_MIN_H);
                crate::ui::chart_render::show_chart(ui, egui::Id::new(("chart_tab", id)), &data, &opts, Some(h));
            });
        }
    }

    /// The Chart tab of the side panel: the chart with a "Pop out" button (only where native windows
    /// work), or, while it is in its own window, a note with "Dock back".
    pub(crate) fn render_chart_tab(&mut self, ui: &mut egui::Ui) {
        let can_pop = !ui.ctx().embed_viewports();
        let tk = *theme::tokens(self.dark_mode);
        let mut toggle = false;
        if self.chart_popped {
            ui.add(egui::Label::new(egui::RichText::new("The chart is in its own window.").color(tk.muted)).wrap());
            let b = ui.button("Dock back");
            self.chart.rects.pop_button = Some(b.rect);
            toggle = b.clicked();
        } else {
            if can_pop {
                let b = ui.button(format!("{}  Pop out", egui_phosphor::regular::ARROW_SQUARE_OUT)).on_hover_text("Show the chart in its own window");
                self.chart.rects.pop_button = Some(b.rect);
                toggle = b.clicked();
            } else {
                self.chart.rects.pop_button = None;
            }
            self.render_chart_body(ui);
        }
        if toggle {
            self.chart_popped = !self.chart_popped;
            if self.chart_popped {
                self.chart_pop_requests += 1;
            }
        }
    }

    /// The chart in its own native window; closing the window (or a backend without windows) docks it back.
    pub(crate) fn render_chart_popout(&mut self, ctx: &egui::Context) {
        if !self.chart_popped || self.tabs.is_empty() {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title("MicroParquet \u{2014} Chart")
            .with_inner_size(theme::CHART_POPOUT_SIZE)
            .with_min_inner_size(theme::CHART_POPOUT_MIN_SIZE)
            .with_resizable(true)
            .with_maximize_button(true)
            .with_minimize_button(true);
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("chart_popout"), builder, |ctx, class| {
            if class == egui::ViewportClass::Embedded || ctx.input(|i| i.viewport().close_requested()) {
                self.chart_popped = false;
                return;
            }
            self.chart_popout_ui(ctx);
        });
    }

    /// What fills the chart's own window (tests draw it in a headless window of its size).
    pub(crate) fn chart_popout_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.render_chart_body(ui);
            });
        });
    }

    fn render_chart_body(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            return;
        }
        self.chart_attach_tab();
        self.chart_tick();
        let tk = *theme::tokens(self.dark_mode);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let avail = tab.available_columns();
        let order_lists = self.sort_lists.lists.clone();
        let mut rects = ChartRects::default();
        let mut open_tab = false;
        let mut open_chart_tab = false;
        let mut show_table = self.chart.show_table;
        let mut spec = self.chart.spec.clone();
        let pop_button = self.chart.rects.pop_button;
        let controls = egui::Grid::new("chart_controls").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
            let mut add = |ui: &mut egui::Ui, name: &str, rects: &mut ChartRects, r: egui::Rect| rects.fields.push((name.to_string(), r));
            let combo_w = theme::CHART_COMBO_W;
            ui.label("Category (X)");
            let r = egui::ComboBox::from_id_salt("chart_category").width(combo_w).selected_text(&spec.category).show_ui(ui, |ui| {
                for c in &avail {
                    ui.selectable_value(&mut spec.category, c.clone(), c);
                }
            });
            add(ui, "Category", &mut rects, r.response.rect);
            ui.end_row();
            ui.label("Series");
            let shown = spec.series.clone().unwrap_or_else(|| "(none)".to_string());
            let r = egui::ComboBox::from_id_salt("chart_series").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut spec.series, None, "(none)");
                for c in avail.iter().filter(|c| **c != spec.category) {
                    ui.selectable_value(&mut spec.series, Some(c.clone()), c);
                }
            });
            add(ui, "Series", &mut rects, r.response.rect);
            ui.end_row();
            ui.label("Aggregation");
            let r = egui::ComboBox::from_id_salt("chart_agg").width(theme::CHART_COMBO_NARROW_W).selected_text(spec.agg.label()).show_ui(ui, |ui| {
                for a in ChartAgg::all() {
                    ui.selectable_value(&mut spec.agg, *a, a.label());
                }
            });
            add(ui, "Aggregation", &mut rects, r.response.rect);
            ui.end_row();
            if spec.agg.needs_value() {
                ui.label("Value");
                let shown = spec.value.clone().unwrap_or_else(|| "(choose)".to_string());
                let r = egui::ComboBox::from_id_salt("chart_value").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                    for c in &avail {
                        ui.selectable_value(&mut spec.value, Some(c.clone()), c);
                    }
                });
                add(ui, "Value", &mut rects, r.response.rect);
                ui.end_row();
            }
            if spec.agg == ChartAgg::WeightedAvg {
                ui.label("by");
                let shown = spec.weight.clone().unwrap_or_else(|| "(choose)".to_string());
                let r = egui::ComboBox::from_id_salt("chart_weight").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                    for c in &avail {
                        ui.selectable_value(&mut spec.weight, Some(c.clone()), c);
                    }
                });
                add(ui, "Weight", &mut rects, r.response.rect);
                ui.end_row();
            }
            ui.label("Type");
            let r = egui::ComboBox::from_id_salt("chart_kind").width(theme::CHART_COMBO_NARROW_W).selected_text(if spec.kind == ChartKind::Bar { "Bar" } else { "Line" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut spec.kind, ChartKind::Bar, "Bar");
                ui.selectable_value(&mut spec.kind, ChartKind::Line, "Line");
            });
            add(ui, "Type", &mut rects, r.response.rect);
            ui.end_row();
            ui.label("Order");
            let order_text = if spec.by_value { "By value".to_string() } else { spec.order.label() };
            let r = egui::ComboBox::from_id_salt("chart_order").width(theme::CHART_COMBO_NARROW_W).selected_text(order_text).show_ui(ui, |ui| {
                if ui.selectable_label(!spec.by_value && spec.order.is_natural(), "Natural").clicked() {
                    spec.order = SortOrder::Natural;
                    spec.by_value = false;
                }
                for l in &order_lists {
                    let o = SortOrder::from_list(l);
                    if ui.selectable_label(!spec.by_value && spec.order == o, &l.name).clicked() {
                        spec.order = o;
                        spec.by_value = false;
                    }
                }
                if ui.selectable_label(!spec.by_value && spec.order == SortOrder::Tenor, "Tenor").clicked() {
                    spec.order = SortOrder::Tenor;
                    spec.by_value = false;
                }
                if ui.selectable_label(spec.by_value, "By value").clicked() {
                    spec.by_value = true;
                }
            });
            add(ui, "Order", &mut rects, r.response.rect);
            ui.end_row();
            ui.label("Top N");
            let r = ui.add(egui::DragValue::new(&mut spec.top_n).range(1..=500).speed(0.2)); // not a size: a drag speed
            add(ui, "Top N", &mut rects, r.rect);
            ui.end_row();
            ui.label("Scale");
            let r = egui::ComboBox::from_id_salt("chart_scale").width(theme::CHART_COMBO_NARROW_W).selected_text(spec.scale.label()).show_ui(ui, |ui| {
                for s in ChartScale::all() {
                    ui.selectable_value(&mut spec.scale, *s, s.label());
                }
            });
            add(ui, "Scale", &mut rects, r.response.rect);
            ui.end_row();
            ui.label("");
            let b = ui.add_enabled(self.chart.data.is_some(), egui::Button::new("Open chart data as tab"));
            add(ui, "Open chart data as tab", &mut rects, b.rect);
            ui.end_row();
            if b.clicked() {
                open_tab = true;
            }
            ui.label("View");
            ui.horizontal(|ui| {
                let b = ui.selectable_label(show_table, "Table");
                add(ui, "Table", &mut rects, b.rect);
                if b.clicked() {
                    show_table = !show_table;
                }
                let b = ui.add_enabled(self.chart.data.is_some(), egui::Button::new("Chart tab"));
                add(ui, "Open as chart tab", &mut rects, b.rect);
                if b.clicked() {
                    open_chart_tab = true;
                }
            });
            ui.end_row();
        });
        rects.controls = Some(controls.response.rect);
        self.chart.show_table = show_table;
        if spec != self.chart.spec {
            self.chart.spec = spec;
            self.chart_save_choices();
            self.chart_tick();
        }
        // the title, then the plot (or why there is none)
        let mut new_map: Option<micro_parquet::charts::colours::ColourMap> = None;
        let scale = self.chart_scale(&self.chart.spec);
        let unit = scale.as_ref().map(scale_word).filter(|u| !u.is_empty()).map(str::to_string);
        match (&self.chart.data, &self.chart.error) {
            (_, Some(e)) => {
                rects.title = Some(ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap()).rect);
            }
            (Some(d), None) => {
                let mut title = d.title(unit.as_deref());
                if self.chart.busy {
                    title.push_str("  \u{2014} calculating\u{2026}");
                }
                rects.title = Some(ui.add(egui::Label::new(egui::RichText::new(title).strong()).wrap()).rect);
                if !d.notes.is_empty() {
                    rects.notes = Some(ui.add(egui::Label::new(egui::RichText::new(d.notes.join(" ")).color(tk.muted).size(theme::TEXT_LABEL)).wrap()).rect);
                }
                if self.chart.show_table {
                    let opts = micro_parquet::charts::layout::LayoutOpts { dark: self.dark_mode, divisor: scale.map(|s| s.divisor).unwrap_or(1.0), decimals: self.global_decimals, unit: String::new(), bare: true, colours: None };
                    crate::ui::chart_render::show_table(ui, d, &opts);
                } else {
                    let (plot, map) = self.draw_chart_plot(ui, d, scale);
                    rects.plot = Some(plot);
                    new_map = Some(map);
                }
            }
            (None, None) => {
                rects.title = Some(ui.label(egui::RichText::new("Calculating the chart\u{2026}").color(tk.muted)).rect);
            }
        }
        rects.pop_button = pop_button;
        self.chart.rects = rects;
        if let Some(m) = new_map.filter(|m| *m != self.chart.spec.colour_map) {
            self.chart.spec.colour_map = m.clone();
            if let Some(d) = self.chart.data.as_mut() {
                d.spec.colour_map = m;
            }
            self.chart_save_choices();
        }
        if open_tab {
            self.chart_open_as_tab();
        }
        if open_chart_tab {
            self.chart_open_as_chart_tab();
        }
    }

    /// The tooltip of one bar or point: the category (whole), the series, the value in the chart's
    /// format and the number of rows behind it.
    pub(crate) fn chart_tip(&self, d: &ChartData, j: usize, i: usize) -> String {
        // (what is calculated comes from the data; how it is shown from the chart's current choice)
        let mut shown = d.spec.clone();
        shown.scale = self.chart.spec.scale;
        let spec = &d.spec;
        let v = d.values.get(j).and_then(|r| r.get(i)).copied().flatten().map(|v| self.chart_value_text(&shown, v)).unwrap_or_else(|| "no value".to_string());
        let series = match &spec.series {
            Some(name) => format!("\n{}: {}", name, d.series.get(j).cloned().unwrap_or_default()),
            None => String::new(),
        };
        let rows = d.rows.get(j).and_then(|r| r.get(i)).copied().unwrap_or(0);
        format!("{}: {}{}\n{}\n{} rows", spec.category, d.categories.get(i).cloned().unwrap_or_default(), series, v, format_with_commas(rows as f64, 0))
    }

    /// The chart: drawn by the Phase 7 renderer (`chart_render`) in the quick-chart layout (plot and
    /// legend; the title line and notes above are the Phase 6 ones). Returns the chart's rectangle and
    /// the colour map with every series in it, which the caller stores in the spec.
    fn draw_chart_plot(&self, ui: &mut egui::Ui, d: &ChartData, scale: Option<MoneyScale>) -> (egui::Rect, micro_parquet::charts::colours::ColourMap) {
        let opts = micro_parquet::charts::layout::LayoutOpts {
            dark: self.dark_mode,
            divisor: scale.map(|s| s.divisor).unwrap_or(1.0),
            decimals: self.global_decimals,
            unit: scale.as_ref().map(scale_word).unwrap_or("").to_string(),
            bare: true,
            colours: Some(self.chart.spec.colour_map.clone()),
        };
        // what is drawn follows the live choices; the numbers are the calculated ones
        let live = &self.chart.spec;
        let shown;
        let data = if d.spec == *live {
            d
        } else {
            let mut c = d.clone();
            c.spec = live.clone();
            shown = c;
            &shown
        };
        let out = crate::ui::chart_render::show_chart(ui, egui::Id::new("chart_plot"), data, &opts, None);
        let plot = out.layout.panels.first().map(|p| p.plot).unwrap_or(out.rect);
        (plot, out.layout.colour_map)
    }
}
