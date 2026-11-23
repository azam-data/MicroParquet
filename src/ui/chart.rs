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
    /// What the last HTML export or preview said (true = a problem).
    pub export_message: Option<(bool, String)>,
    /// Tests: write here instead of asking for a file, and never start the browser.
    pub export_path_override: Option<std::path::PathBuf>,
    pub no_browser: bool,
    /// The last script "Copy as Python" put on the clipboard (tests read it; the clipboard itself is not readable).
    pub copied_python: Option<String>,
    /// "Reset colours" was clicked while the numbers were being calculated again: it is done when they arrive, on
    /// the names they have (the names of the old numbers are about to go).
    pub reset_colours: bool,
}

/// The names a chart's colours go to: its series, or its categories when it has no series.
pub(crate) fn colour_names(d: &ChartData, by_series: bool) -> Vec<String> {
    if by_series {
        d.series.iter().filter(|s| !s.is_empty()).cloned().collect()
    } else {
        d.categories.clone()
    }
}

/// What the studio's buttons asked for this frame.
#[derive(Default)]
pub(crate) struct StudioActions {
    pub open_tab: bool,
    pub open_chart_tab: bool,
    pub save: bool,
}

/// A chart that is a tab of its own ("Open as tab"): what it shows and in what scale.
#[derive(Clone, Debug)]
pub struct ChartTabInfo {
    pub data: ChartData,
    pub show_table: bool,
    pub divisor: f64,
    pub decimals: usize,
    pub unit: String,
    /// The id of the tab the chart draws (a project writes it as the chart's source, Phase 8 W4).
    pub source_tab: Option<u64>,
}

impl Default for ChartState {
    fn default() -> Self {
        Self { spec: ChartSpec::default(), tab_id: None, tab_key: String::new(), data: None, error: None, busy: false, reset_colours: false, token: 0, submitted: None, saved: String::new(), rects: ChartRects::default(), show_table: false, export_message: None, export_path_override: None, no_browser: false, copied_python: None }
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
pub(crate) fn tab_key(tab: &TabState) -> String {
    micro_parquet::steps_edit::source_files(&tab.source).first().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| tab.title.clone())
}

fn is_number_type(dt: &polars::prelude::DataType) -> bool {
    dt.is_numeric() || matches!(dt, polars::prelude::DataType::Decimal(..))
}

impl MicroParquetApp {
    /// The scale the chart shows its values in (`None` = as they are), from the value column's
    /// display format or the chart's own choice.
    pub(crate) fn chart_scale(&self, spec: &ChartSpec) -> Option<MoneyScale> {
        self.chart_scale_of(spec, self.tabs.get(self.active_tab))
    }

    /// The same for the chart of tab `tab` (a project opens charts of tabs that are not the active one).
    pub(crate) fn chart_scale_of(&self, spec: &ChartSpec, tab: Option<&TabState>) -> Option<MoneyScale> {
        if spec.agg == ChartAgg::Count {
            return None;
        }
        match spec.scale.fixed() {
            Some(s) => s,
            None => {
                let col = spec.value.as_deref()?;
                let fmt = tab.and_then(|t| t.col_formats.get(col));
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
        let pending = self.chart_pending.remove(&tab.id);
        let spec = match saved {
            // a template's chart for this tab (it was opened for it)
            _ if pending.is_some() => pending.unwrap_or_default(),
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
                if self.chart.reset_colours {
                    self.chart.reset_colours = false;
                    self.chart_reset_colours_now();
                }
            }
            Err(e) => {
                self.chart.data = None;
                self.chart.error = Some(e);
                self.chart.reset_colours = false;
            }
        }
    }

    /// "Reset colours": the names of the chart as it is now take the slots 1, 2, 3 … in their order. While the numbers
    /// are being calculated again the names are the old ones, so the reset waits for the new numbers. On a chart tab
    /// the chart that is shown (the tab's own snapshot) is reset as well.
    pub(crate) fn chart_reset_colours(&mut self) {
        self.chart_tab_reset_colours();
        if self.chart.busy {
            self.chart.reset_colours = true;
        } else {
            self.chart_reset_colours_now();
        }
    }

    fn chart_reset_colours_now(&mut self) {
        let Some(d) = self.chart.data.as_ref() else {
            return;
        };
        let mut map = micro_parquet::charts::colours::ColourMap::default();
        map.assign_all(&colour_names(d, self.chart.spec.series.is_some()));
        if map == self.chart.spec.colour_map {
            return;
        }
        self.chart.spec.colour_map = map.clone();
        if let Some(d) = self.chart.data.as_mut() {
            d.spec.colour_map = map;
        }
        self.chart_save_choices();
    }

    /// On a chart tab the chart that is shown is the tab's own: its colours are reset on its own names.
    fn chart_tab_reset_colours(&mut self) {
        let Some(id) = self.tabs.get(self.active_tab).map(|t| t.id) else { return };
        let title = self.tabs[self.active_tab].title.clone();
        let Some(info) = self.chart_tabs.get_mut(&id) else { return };
        let mut map = micro_parquet::charts::colours::ColourMap::default();
        map.assign_all(&colour_names(&info.data, info.data.spec.series.is_some()));
        if map == info.data.spec.colour_map {
            return;
        }
        info.data.spec.colour_map = map;
        // kept with the session, with the chart's other settings
        if let Ok(spec) = serde_json::to_value(&info.data.spec) {
            let mut changed = false;
            for e in self.settings.chart_tabs.iter_mut() {
                if e.get("title").and_then(|t| t.as_str()) == Some(title.as_str()) {
                    e["spec"] = spec.clone();
                    changed = true;
                }
            }
            if changed {
                self.save_settings();
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

    /// How the value column of a chart's table is shown in its own tab: the scale the chart showed it in. The numbers
    /// in the table are always absolute (BDT); only this format divides them, once, on display, so a chart of that
    /// tab (or of its Sum column) never divides a second time.
    pub(crate) fn chart_data_formats(&self, data: &ChartData) -> HashMap<String, ColumnFormatSpec> {
        use micro_parquet::models::MetricFormatType as F;
        let mut m = HashMap::new();
        if data.spec.agg == ChartAgg::Count {
            return m;
        }
        let fmt_type = match self.chart_scale(&data.spec) {
            Some(s) => match s.suffix {
                "Cr" => Some(F::BdtCrore),
                "L" => Some(F::BdtLakh),
                "M" => Some(F::BdtMillion),
                "K" => Some(F::BdtThousand),
                _ => None,
            },
            // "Plain (BDT)" chosen on the chart: the table shows the numbers as they are
            None if data.spec.scale == ChartScale::Plain => Some(F::StandardCommas),
            None => None,
        };
        if let Some(fmt_type) = fmt_type {
            m.insert(micro_parquet::chart::chart_value_name(data), ColumnFormatSpec { fmt_type, decimals: self.global_decimals });
        }
        m
    }

    /// "Open chart data as tab": the table behind the chart, in a new tab that can be exported.
    pub(crate) fn chart_open_as_tab(&mut self) {
        let Some(mut data) = self.chart.data.clone() else {
            self.status_message = "There is no chart data yet.".to_string();
            return;
        };
        // the look settings (scale, title, colours) are the live ones: the numbers were calculated before they changed
        data.spec = self.chart.spec.clone();
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
                let formats = self.chart_data_formats(&data);
                self.create_tab_from_source(title, src, Vec::new(), formats);
            }
            Err(e) => self.status_message = format!("The chart data could not be opened as a tab: {:#}", e),
        }
    }

    /// "Open as tab": the chart in a tab of its own that fills the main area. The tab is the chart's data
    /// table (so it can be exported like any tab); the main area draws the chart, with a Table toggle.
    pub(crate) fn chart_open_as_chart_tab(&mut self) {
        let Some(mut data) = self.chart.data.clone() else {
            self.status_message = "There is no chart data yet.".to_string();
            return;
        };
        data.spec = self.chart.spec.clone();
        let scale = self.chart_scale(&self.chart.spec);
        let info = ChartTabInfo {
            data: data.clone(),
            show_table: false,
            divisor: scale.map(|s| s.divisor).unwrap_or(1.0),
            decimals: self.global_decimals,
            unit: scale.as_ref().map(scale_word).unwrap_or("").to_string(),
            source_tab: self.chart.tab_id,
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
                let formats = self.chart_data_formats(&data);
                let id = self.create_tab_from_source_with(title.clone(), src, Vec::new(), formats, None);
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
            self.render_chart_studio(ui, true);
        });
    }

    fn render_chart_body(&mut self, ui: &mut egui::Ui) {
        self.render_chart_studio(ui, false);
    }

    /// The chart studio: the controls and the chart. `wide` puts the controls in a column beside the
    /// chart (the pop-out window); otherwise the chart is under the controls (the side panel).
    pub(crate) fn render_chart_studio(&mut self, ui: &mut egui::Ui, wide: bool) {
        if self.tabs.is_empty() {
            return;
        }
        self.chart_attach_tab();
        self.chart_tick();
        let pop_button = self.chart.rects.pop_button;
        let mut rects = ChartRects::default();
        let mut acts = StudioActions::default();
        if wide {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(theme::CHART_CONTROLS_W, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::ScrollArea::vertical().id_salt("studio_controls").auto_shrink([false, false]).show(ui, |ui| {
                        self.chart_controls(ui, &mut rects, &mut acts);
                        self.chart_sections(ui, &mut rects);
                    });
                });
                ui.vertical(|ui| {
                    self.chart_plot_area(ui, &mut rects);
                });
            });
        } else {
            self.chart_controls(ui, &mut rects, &mut acts);
            self.chart_plot_area(ui, &mut rects);
            // under the chart: shape, titles, reference lines, measures (the chart stays where it was)
            ui.add_space(theme::SPACE_S);
            self.chart_sections(ui, &mut rects);
        }
        rects.pop_button = pop_button;
        let old = std::mem::take(&mut self.chart.rects);
        // the controls and the plot each fill their own part of the rectangles
        self.chart.rects = ChartRects { title: rects.title.or(old.title), plot: rects.plot, notes: rects.notes, ..rects };
        if acts.open_tab {
            self.chart_open_as_tab();
        }
        if acts.open_chart_tab {
            self.chart_open_as_chart_tab();
        }
        if acts.save {
            self.open_chart_save();
        }
    }

    /// The controls: the Phase 6 grid (category, series, aggregation, value, type, order, Top N, scale),
    /// the buttons, and the studio sections.
    fn chart_controls(&mut self, ui: &mut egui::Ui, rects: &mut ChartRects, acts: &mut StudioActions) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let avail = tab.available_columns();
        let numeric: Vec<String> = tab.cached_slice.get_columns().iter().filter(|c| is_number_type(c.dtype())).map(|c| c.name().to_string()).collect();
        let order_lists = self.sort_lists.lists.clone();
        let builtin = self.builtin_orders();
        let mut show_table = self.chart.show_table;
        let mut spec = self.chart.spec.clone();
        let parts = self.chart_date_parts();
        let controls = egui::Grid::new("chart_controls").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
            let mut add = |_ui: &mut egui::Ui, name: &str, rects: &mut ChartRects, r: egui::Rect| rects.fields.push((name.to_string(), r));
            let combo_w = theme::CHART_COMBO_W;
            ui.label("Category (X)");
            let r = egui::ComboBox::from_id_salt("chart_category").width(combo_w).height(theme::CHART_PICKER_POPUP_H).selected_text(&spec.category).show_ui(ui, |ui| {
                for c in &avail {
                    ui.selectable_value(&mut spec.category, c.clone(), c);
                }
                // W2: the parts of each date column, "Open Date › Quarter"
                if let Some(f) = date_part_items(ui, &parts, Some(spec.category.as_str())) {
                    spec.category = f.name.clone();
                    spec.order = f.sort_order().unwrap_or_default();
                    spec.by_value = false;
                    spec.category_part = Some(f);
                }
            });
            add(ui, "Category", rects, r.response.rect);
            ui.end_row();
            ui.label("Series");
            let shown = spec.series.clone().unwrap_or_else(|| "(none)".to_string());
            let r = egui::ComboBox::from_id_salt("chart_series").width(combo_w).height(theme::CHART_PICKER_POPUP_H).selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut spec.series, None, "(none)");
                for c in avail.iter().filter(|c| **c != spec.category) {
                    ui.selectable_value(&mut spec.series, Some(c.clone()), c);
                }
                if let Some(f) = date_part_items(ui, &parts, spec.series.as_deref()) {
                    spec.series = Some(f.name.clone());
                    spec.series_part = Some(f);
                }
            });
            add(ui, "Series", rects, r.response.rect);
            ui.end_row();
            ui.label("Aggregation");
            let r = egui::ComboBox::from_id_salt("chart_agg").width(theme::CHART_COMBO_NARROW_W).selected_text(spec.agg.label()).show_ui(ui, |ui| {
                for a in ChartAgg::all() {
                    ui.selectable_value(&mut spec.agg, *a, a.label());
                }
            });
            add(ui, "Aggregation", rects, r.response.rect);
            ui.end_row();
            if spec.agg.needs_value() {
                ui.label("Value");
                let shown = spec.value.clone().unwrap_or_else(|| "(choose)".to_string());
                let r = egui::ComboBox::from_id_salt("chart_value").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                    for c in &avail {
                        ui.selectable_value(&mut spec.value, Some(c.clone()), c);
                    }
                });
                add(ui, "Value", rects, r.response.rect);
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
                add(ui, "Weight", rects, r.response.rect);
                ui.end_row();
            }
            ui.label("Type");
            let r = egui::ComboBox::from_id_salt("chart_kind").width(combo_w).selected_text(spec.kind.label()).show_ui(ui, |ui| {
                for k in ChartKind::all() {
                    ui.selectable_value(&mut spec.kind, *k, k.label());
                }
            });
            add(ui, "Type", rects, r.response.rect);
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
                for o in &builtin {
                    if ui.selectable_label(!spec.by_value && spec.order == *o, o.label()).clicked() {
                        spec.order = o.clone();
                        spec.by_value = false;
                    }
                }
                if ui.selectable_label(spec.by_value, "By value").clicked() {
                    spec.by_value = true;
                }
            });
            add(ui, "Order", rects, r.response.rect);
            ui.end_row();
            if spec.kind == ChartKind::Donut {
                // a donut draws every part; "Group small parts" keeps the N largest and puts the rest in "Other"
                ui.label("Group small parts");
                ui.horizontal(|ui| {
                    let r = egui::ComboBox::from_id_salt("chart_donut_group").width(theme::CHART_COMBO_NARROW_W).selected_text(if spec.donut_top.is_some() { "Top N + Other" } else { "Off" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut spec.donut_top, None, "Off");
                        if ui.selectable_label(spec.donut_top.is_some(), "Top N + Other").clicked() && spec.donut_top.is_none() {
                            spec.donut_top = Some(4);
                        }
                    });
                    add(ui, "Group small parts", rects, r.response.rect);
                    if let Some(n) = spec.donut_top.as_mut() {
                        let r = ui.add(egui::DragValue::new(n).range(2..=12).speed(0.2)); // not a size: a drag speed
                        add(ui, "Group N", rects, r.rect);
                    }
                });
                ui.end_row();
            } else {
                ui.label("Top N");
                let r = ui.add(egui::DragValue::new(&mut spec.top_n).range(1..=500).speed(0.2)); // not a size: a drag speed
                add(ui, "Top N", rects, r.rect);
                ui.end_row();
            }
            ui.label("Scale");
            let r = egui::ComboBox::from_id_salt("chart_scale").width(theme::CHART_COMBO_NARROW_W).selected_text(spec.scale.label()).show_ui(ui, |ui| {
                for s in ChartScale::all() {
                    ui.selectable_value(&mut spec.scale, *s, s.label());
                }
            });
            add(ui, "Scale", rects, r.response.rect);
            ui.end_row();
            ui.label("");
            let b = ui.add_enabled(self.chart.data.is_some(), egui::Button::new("Open chart data as tab"));
            add(ui, "Open chart data as tab", rects, b.rect);
            ui.end_row();
            if b.clicked() {
                acts.open_tab = true;
            }
            ui.label("View");
            ui.horizontal(|ui| {
                let b = ui.selectable_label(show_table, "Table");
                add(ui, "Table", rects, b.rect);
                if b.clicked() {
                    show_table = !show_table;
                }
                let b = ui.add_enabled(self.chart.data.is_some(), egui::Button::new("Chart tab"));
                add(ui, "Open as chart tab", rects, b.rect);
                if b.clicked() {
                    acts.open_chart_tab = true;
                }
            });
            ui.end_row();
        });
        rects.controls = Some(controls.response.rect);
        self.chart.show_table = show_table;
        // a part left behind when another column is picked is dropped (W2)
        spec.drop_stale_date_parts();
        // W0 auto order: a category picked here with no order chosen gets the steps' order, Calendar or Ranges
        let new_category = spec.category != self.chart.spec.category && !spec.category.is_empty() && spec.category_part.is_none();
        let auto = new_category && spec.order.is_natural() && !spec.by_value;
        if spec != self.chart.spec {
            let category = spec.category.clone();
            self.auto_series_facet_order(&mut spec);
            self.chart_set_spec(spec);
            if auto {
                if let Some(order) = self.auto_order(&category, crate::ui::auto_order::AutoTarget::ChartCategory) {
                    let mut s = self.chart.spec.clone();
                    s.order = order;
                    self.chart_set_spec(s);
                }
            }
        }
    }

    /// The studio sections (shape, titles and look, reference lines, measures and panels).
    fn chart_sections(&mut self, ui: &mut egui::Ui, rects: &mut ChartRects) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let avail = tab.available_columns();
        let numeric: Vec<String> = tab.cached_slice.get_columns().iter().filter(|c| is_number_type(c.dtype())).map(|c| c.name().to_string()).collect();
        let mut spec = self.chart.spec.clone();
        ui.horizontal(|ui| {
            let b = ui.add_enabled(self.chart.data.is_some(), egui::Button::new("Save chart\u{2026}"));
            rects.fields.push(("Save chart".to_string(), b.rect));
            if b.clicked() {
                self.open_chart_save();
            }
            let b = ui.button("Open saved chart\u{2026}");
            rects.fields.push(("Open saved chart".to_string(), b.rect));
            if b.clicked() {
                self.open_chart_open();
            }
        });
        ui.horizontal_wrapped(|ui| {
            let ready = self.chart.data.is_some() && !self.chart.busy;
            let b = ui.add_enabled(ready, egui::Button::new("Export to HTML\u{2026}"));
            rects.fields.push(("Export to HTML".to_string(), b.rect));
            if b.clicked() {
                self.chart_export_html();
            }
            let b = ui.add_enabled(ready, egui::Button::new("Export to Excel\u{2026}"));
            rects.fields.push(("Export to Excel".to_string(), b.rect));
            if b.clicked() {
                self.chart_export_xlsx();
            }
            let b = ui.add_enabled(ready, egui::Button::new("Preview in browser"));
            rects.fields.push(("Preview in browser".to_string(), b.rect));
            if b.clicked() {
                let open = !self.chart.no_browser;
                self.chart_preview_in_browser(open);
            }
            let b = ui.add_enabled(ready, egui::Button::new("Copy as Python"));
            rects.fields.push(("Copy as Python".to_string(), b.rect));
            if b.clicked() {
                self.chart_copy_as_python(ui.ctx());
            }
        });
        if let Some((bad, m)) = self.chart.export_message.clone() {
            let tk = *theme::tokens(self.dark_mode);
            ui.add(egui::Label::new(egui::RichText::new(m).size(theme::TEXT_LABEL).color(if bad { tk.danger } else { tk.success })).wrap());
        }
        self.studio_sections(ui, &mut spec, &avail, &numeric, rects);
        if spec != self.chart.spec {
            self.auto_series_facet_order(&mut spec);
            self.chart_set_spec(spec);
        }
    }

    /// The title line (what was calculated), then the chart or its table (or why there is none).
    fn chart_plot_area(&mut self, ui: &mut egui::Ui, rects: &mut ChartRects) {
        let tk = *theme::tokens(self.dark_mode);
        let mut new_map: Option<micro_parquet::charts::colours::ColourMap> = None;
        let mut labels_dont_fit = false;
        let mut switch_horizontal: Option<ChartSpec> = None;
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
                rects.title = Some(ui.add(egui::Label::new(egui::RichText::new(title).color(tk.ink2).size(theme::TEXT_LABEL)).wrap()).rect);
                if self.chart.show_table {
                    let opts = micro_parquet::charts::layout::LayoutOpts { dark: self.dark_mode, divisor: scale.map(|s| s.divisor).unwrap_or(1.0), decimals: self.global_decimals, unit: String::new(), bare: true, colours: None };
                    crate::ui::chart_render::show_table(ui, d, &opts);
                } else {
                    let (plot, map, dont_fit) = self.draw_chart_plot(ui, d, scale);
                    rects.plot = Some(plot);
                    new_map = Some(map);
                    labels_dont_fit = dont_fit;
                }
            }
            (None, None) => {
                rects.title = Some(ui.label(egui::RichText::new("Calculating the chart\u{2026}").color(tk.muted)).rect);
            }
        }
        // a donut of many parts: one line of advice
        if let (ChartKind::Donut, Some(d)) = (self.chart.spec.kind, self.chart.data.as_ref()) {
            let parts = d.categories.len();
            if parts > 6 {
                let l = ui.add(egui::Label::new(egui::RichText::new("Many parts are hard to compare; a sorted bar may read better").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                rects.fields.push(("Many parts".to_string(), l.rect));
            }
        }
        // the category names run into each other even angled: say so, and offer horizontal bars in one click
        if labels_dont_fit {
            let vertical_bars = self.chart.spec.kind == ChartKind::Bar && !self.chart.spec.horizontal;
            let text = if vertical_bars { "Labels don't fit; try horizontal bars." } else { "Labels don't fit; use Top N or a filter to show fewer." };
            ui.horizontal(|ui| {
                let l = ui.add(egui::Label::new(egui::RichText::new(text).size(theme::TEXT_LABEL).color(tk.warning)).wrap());
                rects.fields.push(("Labels don't fit".to_string(), l.rect));
                if vertical_bars {
                    let b = ui.button("Horizontal bars");
                    rects.fields.push(("Switch to horizontal bars".to_string(), b.rect));
                    if b.clicked() {
                        let mut spec = self.chart.spec.clone();
                        spec.horizontal = true;
                        switch_horizontal = Some(spec);
                    }
                }
            });
        }
        if let Some(spec) = switch_horizontal {
            self.chart_set_spec(spec);
        }
        // the map the drawing filled in is kept only when it was drawn from the numbers of these settings: while they
        // are being calculated again the chart shows the old names, which must not take the slots of the new ones
        let fresh = !self.chart.busy && self.chart.data.as_ref().is_some_and(|d| d.spec.calc_key() == self.chart.spec.calc_key());
        if let Some(m) = new_map.filter(|m| fresh && *m != self.chart.spec.colour_map) {
            self.chart.spec.colour_map = m.clone();
            if let Some(d) = self.chart.data.as_mut() {
                d.spec.colour_map = m;
            }
            self.chart_save_choices();
        }
    }

    /// Makes `spec` the chart's settings: saved for the tab, and the numbers recalculated when they changed.
    pub(crate) fn chart_set_spec(&mut self, mut spec: ChartSpec) {
        // the colours belong to the category and series columns: new columns, new names, slots 1, 2, 3 again
        // (a chart that brings its own colour map, a saved chart or a template, keeps it)
        if spec.colour_map == self.chart.spec.colour_map {
            spec.reset_colours_if_columns_changed(&self.chart.spec);
        }
        self.chart.spec = spec;
        self.chart_save_choices();
        self.chart_tick();
    }

    /// The calculated chart with the live look settings, and how its numbers are written, for the HTML page.
    pub(crate) fn chart_export_parts(&self) -> Result<(ChartData, micro_parquet::charts::layout::LayoutOpts), String> {
        if self.chart.busy {
            return Err("The chart is still being calculated. Wait for it to finish, then try again.".to_string());
        }
        let Some(d) = self.chart.data.as_ref() else {
            return Err("There is no chart to write yet. Fix the problem shown above the chart first.".to_string());
        };
        let scale = self.chart_scale(&self.chart.spec);
        let opts = micro_parquet::charts::layout::LayoutOpts {
            dark: false,
            divisor: scale.map(|s| s.divisor).unwrap_or(1.0),
            decimals: self.global_decimals,
            unit: scale.as_ref().map(scale_word).unwrap_or("").to_string(),
            bare: false,
            colours: Some(self.chart.spec.colour_map.clone()),
        };
        let mut data = d.clone();
        data.spec = self.chart.spec.clone();
        Ok((data, opts))
    }

    /// Export ▸ "Chart to HTML…": asks where, then writes the one file.
    pub(crate) fn chart_export_html(&mut self) {
        self.chart_attach_tab_pub();
        let data = match self.chart_export_parts() {
            Ok((d, _)) => d,
            Err(e) => {
                self.chart.export_message = Some((true, e.clone()));
                self.status_message = e;
                return;
            }
        };
        let name = format!("{}.html", micro_parquet::charts::echarts::file_stem(&micro_parquet::charts::layout::chart_title(&data)));
        let picked = match self.chart.export_path_override.clone() {
            Some(p) => Some(p),
            None => rfd::FileDialog::new().add_filter("HTML page", &["html"]).set_file_name(&name).set_title("Save the chart as a web page").save_file(),
        };
        let Some(path) = picked else {
            return;
        };
        self.chart_write_html_to(&path);
    }

    /// Export ▸ "Chart to Excel…": the chart's numbers on a sheet with a native Excel chart beside them.
    pub(crate) fn chart_export_xlsx(&mut self) {
        self.chart_attach_tab_pub();
        let data = match self.chart_export_parts() {
            Ok((d, _)) => d,
            Err(e) => {
                self.chart.export_message = Some((true, e.clone()));
                self.status_message = e;
                return;
            }
        };
        let name = format!("{}.xlsx", micro_parquet::charts::echarts::file_stem(&micro_parquet::charts::layout::chart_title(&data)));
        let picked = match self.chart.export_path_override.clone() {
            Some(p) => Some(p),
            None => rfd::FileDialog::new().add_filter("Excel workbook", &["xlsx"]).set_file_name(&name).set_title("Save the chart as an Excel workbook").save_file(),
        };
        let Some(path) = picked else {
            return;
        };
        self.chart_write_xlsx_to(&path);
    }

    /// Writes the workbook to `path` and says so (or why not) in the studio and the status line.
    pub(crate) fn chart_write_xlsx_to(&mut self, path: &std::path::Path) -> bool {
        let r = self.chart_export_parts().and_then(|(d, o)| micro_parquet::charts::xlsx::write_chart_xlsx(path, &d, &o).map_err(|e| format!("{:#}", e)));
        match r {
            Ok(info) => {
                let extra = info.note.map(|n| format!(" {}", n)).unwrap_or_default();
                let m = format!("Chart written to {}: the numbers on the sheet '{}' and {} native chart(s) beside them.{}", path.display(), info.sheet, info.charts, extra);
                self.chart.export_message = Some((false, m.clone()));
                self.status_message = m;
                true
            }
            Err(e) => {
                self.chart.export_message = Some((true, e.clone()));
                self.status_message = e;
                false
            }
        }
    }

    /// Writes the chart's page to `path` and says so (or why not) in the studio and the status line.
    pub(crate) fn chart_write_html_to(&mut self, path: &std::path::Path) -> bool {
        let r = self.chart_export_parts().and_then(|(d, o)| micro_parquet::charts::echarts::write_chart_html(path, &d, &o).map_err(|e| format!("{:#}", e)));
        match r {
            Ok(()) => {
                let m = format!("Chart written to {}. Open it in any browser; it needs no internet.", path.display());
                self.chart.export_message = Some((false, m.clone()));
                self.status_message = m;
                true
            }
            Err(e) => {
                self.chart.export_message = Some((true, e.clone()));
                self.status_message = e;
                false
            }
        }
    }

    /// "Preview in browser": the page is written to a temporary file and opened with the default browser
    /// (nothing goes over the network). `open` is false in tests.
    pub(crate) fn chart_preview_in_browser(&mut self, open: bool) -> Option<std::path::PathBuf> {
        let r = self.chart_export_parts().and_then(|(d, o)| micro_parquet::charts::echarts::write_preview(&d, &o, &self.paths.temp_dir).map_err(|e| format!("{:#}", e)));
        let path = match r {
            Ok(p) => p,
            Err(e) => {
                self.chart.export_message = Some((true, e.clone()));
                self.status_message = e;
                return None;
            }
        };
        if open {
            if let Err(e) = micro_parquet::charts::echarts::open_in_browser(&path) {
                let m = format!("{:#}", e);
                self.chart.export_message = Some((true, m.clone()));
                self.status_message = m;
                return Some(path);
            }
        }
        let m = format!("Preview opened in your browser from {} (a temporary file on this computer).", path.display());
        self.chart.export_message = Some((false, m.clone()));
        self.status_message = m;
        Some(path)
    }

    /// "Copy as Python": a self-contained script (the chart's data as a pandas DataFrame, the drawing in
    /// matplotlib, or plotly for a Sankey and a treemap) goes to the clipboard.
    pub(crate) fn chart_copy_as_python(&mut self, ctx: &egui::Context) {
        let r = self.chart_export_parts().and_then(|(d, o)| micro_parquet::charts::python::chart_to_python(&d, &o).map_err(|e| format!("{:#}", e)));
        match r {
            Ok(script) => {
                let lines = script.lines().count();
                let plotly = script.contains("import plotly");
                self.chart.copied_python = Some(script.clone());
                ctx.copy_text(script);
                let m = format!("Copied a Python script of {} lines ({}). Paste it into a .py file or a notebook.", lines, if plotly { "plotly: pip install plotly pandas" } else { "matplotlib: pip install pandas matplotlib" });
                self.chart.export_message = Some((false, m.clone()));
                self.status_message = m;
            }
            Err(e) => {
                self.chart.export_message = Some((true, e.clone()));
                self.status_message = e;
            }
        }
    }

    pub(crate) fn chart_attach_tab_pub(&mut self) {
        self.chart_attach_tab();
    }

    /// Charts ▸ "Chart this result": the banking template that fits the active result tab is applied (the
    /// studio opens with it, every setting still editable); with several (or no) fitting templates the
    /// gallery's "Banking templates" is shown.
    pub(crate) fn chart_this_result(&mut self) {
        use micro_parquet::charts::templates::detect;
        let found = self.tabs.get(self.active_tab).map(|t| detect(&t.cached_slice)).unwrap_or_default();
        match found.as_slice() {
            [one] => match self.chart_apply_template(*one) {
                Ok(m) => self.status_message = m,
                Err(e) => self.status_message = e,
            },
            [] => {
                self.chart_attach_tab();
                self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
                self.status_message = "No banking template fits this tab, so the studio is open on it. Charts \u{25b8} New chart \u{25b8} Banking templates lists them.".to_string();
            }
            _ => self.open_chart_gallery(),
        }
    }

    /// The scale and thresholds a template's finding is written with.
    pub(crate) fn chart_template_ctx(&self, t: micro_parquet::charts::templates::Template) -> micro_parquet::charts::templates::Ctx {
        use micro_parquet::charts::templates::{Ctx, Template};
        let mut ctx = Ctx { decimals: self.global_decimals, ..Ctx::default() };
        let tab = self.tabs.get(self.active_tab);
        if t == Template::NiiBridge {
            let col = "Volume effect";
            let fmt = tab.and_then(|t| t.col_formats.get(col));
            if let Some(s) = micro_parquet::models::resolved_scale(col, fmt, self.num_format) {
                ctx.divisor = s.divisor;
                ctx.suffix = s.suffix.to_string();
            }
        }
        if let Some(TabSource::DqScorecard { rules, .. }) = tab.map(|t| &t.source) {
            ctx.dq_green = Some(rules.green_min);
            ctx.dq_amber = Some(rules.amber_min);
        }
        ctx
    }

    /// Applies a banking template to the active tab: its table (when the chart needs numbers the tab does not
    /// have as columns) opens as a tab of its own, and the studio shows the template's chart with the
    /// finding as its title. Returns the message for the status line.
    pub(crate) fn chart_apply_template(&mut self, t: micro_parquet::charts::templates::Template) -> Result<String, String> {
        use micro_parquet::charts::templates::{prepare, yield_curves, CurveInput, Template};
        let ctx = self.chart_template_ctx(t);
        let prepared = if t == Template::YieldCurves {
            let store = micro_parquet::curves::CurveStore::load(&micro_parquet::curves::curves_dir());
            let inputs: Vec<CurveInput> = store.curves.iter().map(CurveInput::from).collect();
            yield_curves(&inputs)
        } else {
            let Some(tab) = self.tabs.get(self.active_tab) else {
                return Err("Open a tab first.".to_string());
            };
            if matches!(t, Template::ClassifiedTrend | Template::BehaviourProfile) && tab.total_rows > tab.cached_slice.height() {
                return Err(format!(
                    "{}: this tab has {} rows and the template reads them all at once. Make a pivot with one row per month and grade (or per month-end) first, then chart that.",
                    t.name(),
                    format_with_commas(tab.total_rows as f64, 0)
                ));
            }
            prepare(t, &tab.cached_slice, &ctx)
        }
        .map_err(|e| format!("{:#}", e))?;
        self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
        let message = format!("Chart this \u{2014} {}: {}.", t.name(), prepared.finding);
        // a second chart of the same result (HQLA composition, NII effects by key) opens first, so the main chart is the one in front
        if let Some(c) = prepared.companion.clone() {
            self.chart_open_prepared(*c)?;
        }
        self.chart_open_prepared(prepared)?;
        self.status_message = message.clone();
        Ok(message)
    }

    /// Opens one prepared template chart: its table as a tab (when the chart needs numbers the tab does
    /// not have as columns) with the spec applied when the tab has loaded, or the spec on the active tab.
    fn chart_open_prepared(&mut self, prepared: micro_parquet::charts::templates::Prepared) -> Result<(), String> {
        match prepared.table {
            Some(mut df) => {
                std::fs::create_dir_all(&self.paths.temp_dir).map_err(|e| format!("The chart table could not be written: {}", e))?;
                let path = self.paths.temp_dir.join(format!("chart_template_{}_{}.parquet", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
                let f = std::fs::File::create(&path).map_err(|e| format!("The chart table could not be written: {}", e))?;
                polars::prelude::ParquetWriter::new(f).finish(&mut df).map_err(|e| format!("The chart table could not be written: {}", e))?;
                let src = TabSource::SingleFile { path, is_parquet: true, config: IngestionConfig::default() };
                // the columns that keep their name keep their display format (Crore, Lakh, …)
                let formats: HashMap<String, ColumnFormatSpec> = self.tabs.get(self.active_tab).map(|tab| tab.col_formats.iter().filter(|(k, _)| df.get_column_names().iter().any(|n| n.as_str() == k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
                let id = self.create_tab_from_source_with(prepared.tab_title.clone(), src, Vec::new(), formats, None);
                self.chart_pending.insert(id, prepared.spec);
            }
            None => {
                self.chart_attach_tab();
                self.chart_set_spec(prepared.spec);
            }
        }
        Ok(())
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
    fn draw_chart_plot(&self, ui: &mut egui::Ui, d: &ChartData, scale: Option<MoneyScale>) -> (egui::Rect, micro_parquet::charts::colours::ColourMap, bool) {
        let opts = micro_parquet::charts::layout::LayoutOpts {
            dark: self.dark_mode,
            divisor: scale.map(|s| s.divisor).unwrap_or(1.0),
            decimals: self.global_decimals,
            unit: scale.as_ref().map(scale_word).unwrap_or("").to_string(),
            bare: false,
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
        (plot, out.layout.colour_map, out.layout.labels_dont_fit)
    }
}

/// The date parts of each Date or Datetime column of a tab (W2), with the calendar in force.
pub(crate) fn date_parts_of(df: &DataFrame, cal: micro_parquet::dates::CalendarSettings) -> Vec<micro_parquet::dates::DateFieldRef> {
    df.get_columns()
        .iter()
        .filter(|c| micro_parquet::dates::is_date_type(c.dtype()))
        .flat_map(|c| micro_parquet::dates::DatePart::ALL.into_iter().map(move |p| micro_parquet::dates::DateFieldRef::new(c.name(), p, cal)))
        .collect()
}

/// The parts listed in a chart picker under their date column ("Open Date › Quarter"); the one clicked.
pub(crate) fn date_part_items(ui: &mut egui::Ui, parts: &[micro_parquet::dates::DateFieldRef], current: Option<&str>) -> Option<micro_parquet::dates::DateFieldRef> {
    let mut picked = None;
    let mut last_col = "";
    for f in parts {
        if f.column != last_col {
            ui.separator();
            last_col = f.column.as_str();
        }
        if ui.selectable_label(current == Some(f.name.as_str()), micro_parquet::dates::picker_label(&f.column, f.part)).clicked() {
            picked = Some(f.clone());
        }
    }
    picked
}

impl MicroParquetApp {
    /// The date parts the chart pickers offer for the active tab.
    pub(crate) fn chart_date_parts(&self) -> Vec<micro_parquet::dates::DateFieldRef> {
        self.tabs.get(self.active_tab).map(|t| date_parts_of(&t.cached_slice, self.calendar())).unwrap_or_default()
    }
}
