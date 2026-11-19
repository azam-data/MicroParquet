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
}

impl Default for ChartState {
    fn default() -> Self {
        Self { spec: ChartSpec::default(), tab_id: None, tab_key: String::new(), data: None, error: None, busy: false, token: 0, submitted: None, saved: String::new(), rects: ChartRects::default() }
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
        });
        rects.controls = Some(controls.response.rect);
        if spec != self.chart.spec {
            self.chart.spec = spec;
            self.chart_save_choices();
            self.chart_tick();
        }
        // the title, then the plot (or why there is none)
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
                rects.plot = Some(self.draw_chart_plot(ui, d, scale));
            }
            (None, None) => {
                rects.title = Some(ui.label(egui::RichText::new("Calculating the chart\u{2026}").color(tk.muted)).rect);
            }
        }
        rects.pop_button = pop_button;
        self.chart.rects = rects;
        if open_tab {
            self.chart_open_as_tab();
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

    /// The bars or lines. The x-axis names the categories (a long name is cut, the tooltip has it
    /// whole), a tooltip gives the category, series, value and number of rows.
    fn draw_chart_plot(&self, ui: &mut egui::Ui, d: &ChartData, scale: Option<MoneyScale>) -> egui::Rect {
        let divisor = scale.map(|s| s.divisor).unwrap_or(1.0);
        let n = d.categories.len();
        let s_n = d.series.len().max(1);
        let spec = d.spec.clone();
        let tip = |j: usize, i: usize| -> String { self.chart_tip(d, j, i) };
        let labels: Vec<String> = d.categories.iter().map(|c| short_label(c, theme::CHART_LABEL_CHARS)).collect();
        let avail_w = ui.available_width();
        let per_label = theme::CHART_LABEL_W;
        let every = ((n as f32 * per_label / avail_w.max(per_label)).ceil() as usize).max(1);
        let mut plot = egui_plot::Plot::new("chart_plot")
            .height(theme::CHART_H)
            .show_axes([true, true])
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .include_y(0.0)
            .x_axis_formatter(move |mark, _| {
                let i = mark.value.round();
                let between = (mark.value - i).abs() > 0.01; // not a size: a mark between two categories
                if between || i < 0.0 || i as usize >= labels.len() {
                    String::new()
                } else {
                    labels[i as usize].clone()
                }
            })
            .x_grid_spacer(move |input| {
                let (lo, hi) = input.bounds;
                let mut marks = Vec::new();
                let mut i = lo.max(0.0).ceil() as i64;
                while (i as f64) <= hi && i < n as i64 {
                    if i as usize % every == 0 {
                        marks.push(egui_plot::GridMark { value: i as f64, step_size: every as f64 });
                    }
                    i += 1;
                }
                marks
            })
            .y_axis_formatter(|mark, _| format_with_commas(mark.value, if mark.step_size >= 1.0 { 0 } else { 2 }));
        if let Some(sc) = scale {
            plot = plot.y_axis_label(scale_word(&sc).to_string());
        }
        if spec.series.is_some() {
            plot = plot.legend(egui_plot::Legend::default());
        }
        if spec.kind == ChartKind::Line {
            let names = d.series.clone();
            let cats = d.categories.clone();
            let vals = d.values.clone();
            let rows = d.rows.clone();
            let texts: Vec<Vec<String>> = (0..s_n).map(|j| (0..n).map(|i| tip(j, i)).collect()).collect();
            let _ = (&cats, &vals, &rows);
            plot = plot.label_formatter(move |name, pt| {
                let i = pt.x.round();
                let j = names.iter().position(|s| s == name).unwrap_or(0);
                if i < 0.0 || i as usize >= n {
                    return String::new();
                }
                texts.get(j).and_then(|t| t.get(i as usize)).cloned().unwrap_or_default()
            });
        }
        let out = plot.show(ui, |plot_ui| {
            for j in 0..s_n {
                let name = d.series.get(j).cloned().unwrap_or_default();
                match spec.kind {
                    ChartKind::Bar => {
                        let w = theme::CHART_GROUP_W / s_n as f64;
                        let bars: Vec<egui_plot::Bar> = (0..n)
                            .filter_map(|i| {
                                let v = d.values[j][i]?;
                                let x = i as f64 + (j as f64 - (s_n as f64 - 1.0) / 2.0) * w;
                                Some(egui_plot::Bar::new(x, v / divisor).width(w * theme::CHART_BAR_FILL).name(tip(j, i)))
                            })
                            .collect();
                        plot_ui.bar_chart(egui_plot::BarChart::new(bars).name(name).element_formatter(Box::new(|bar, _| bar.name.clone())));
                    }
                    // the Phase 6 drawer draws two forms; the other forms are drawn by the Phase 7 renderer (C1)
                    _ => {
                        let pts: Vec<[f64; 2]> = (0..n).filter_map(|i| d.values[j][i].map(|v| [i as f64, v / divisor])).collect();
                        plot_ui.line(egui_plot::Line::new(egui_plot::PlotPoints::from(pts.clone())).name(name.clone()).width(theme::CHART_LINE_W));
                        plot_ui.points(egui_plot::Points::new(egui_plot::PlotPoints::from(pts)).name(name).radius(theme::CHART_POINT_R));
                    }
                }
            }
        });
        out.response.rect
    }
}
