//! Phase 11 F3: the Forecast card of the Analytics pane. It edits the `forecast` of the chart in the Chart tab (the studio's spec);
//! the chart is calculated again by the usual worker and shows the forecast, its bands and the back-test notes. HTML, PDF, PowerPoint,
//! Excel and the dashboard read the chart tab's data, so they show the forecast too once the chart tab is updated.

use crate::ui::prelude::*;
use micro_parquet::charts::spec::ChartKind;
use micro_parquet::forecast::MONTHS;
use micro_parquet::forecast_chart::{parse_month, ForecastShow, ForecastSpec, ModelPick, OverlaySpec};

/// BDT per Crore: adjustments are typed in Crore and kept in BDT, the unit of the numbers.
const CRORE: f64 = 10_000_000.0;

impl MicroParquetApp {
    pub(crate) fn render_forecast_card(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        if self.tabs.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new("Open a table and draw a line chart by month to forecast it.").color(tk.muted)).wrap());
            return;
        }
        self.chart_attach_tab_pub();
        self.chart_tick();
        egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).inner_margin(egui::Margin::same(theme::SPACE_S)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(theme::semibold("Forecast"));
            ui.add(egui::Label::new(egui::RichText::new("Extends the line chart in the Chart tab by 3 to 12 months, with 80% and 95% bands. It is an estimate, never a result.").color(tk.muted)).wrap());
            let mut on = self.chart.spec.forecast.is_some();
            let before = on;
            ui.checkbox(&mut on, "Add a forecast to this chart");
            if on != before {
                self.chart.spec.forecast = on.then(ForecastSpec::default);
            }
            let kind = self.chart.spec.kind;
            let Some(mut f) = self.chart.spec.forecast.clone() else {
                return;
            };
            if kind != ChartKind::Line {
                ui.colored_label(theme::warning(ui), "A forecast is drawn on a line chart.");
                if ui.button("Make it a line chart").clicked() {
                    self.chart.spec.kind = ChartKind::Line;
                }
            }
            egui::Grid::new("forecast_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                ui.label("Months ahead");
                ui.add(egui::DragValue::new(&mut f.horizon).range(3..=12));
                ui.end_row();
                ui.label("Model");
                egui::ComboBox::from_id_salt("forecast_model").selected_text(f.model.label()).show_ui(ui, |ui| {
                    for m in ModelPick::ALL {
                        ui.selectable_value(&mut f.model, m, m.label());
                    }
                });
                ui.end_row();
                if self.chart.spec.series.is_some() {
                    ui.label("Series");
                    egui::ComboBox::from_id_salt("forecast_show").selected_text(if f.show == ForecastShow::Total { "Forecast each, show the total" } else { "Forecast and show each" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.show, ForecastShow::Total, "Forecast each, show the total");
                        ui.selectable_value(&mut f.show, ForecastShow::EachSeries, "Forecast and show each");
                    });
                    ui.end_row();
                }
            });
            ui.checkbox(&mut f.clean_outliers, "Set unusual months aside before fitting").on_hover_text("A month far from its neighbours (and from its usual calendar-month pattern) is flagged below and replaced by what its neighbours say, for the fit only. The chart still shows the real value.");
            // manual adjustments: "+200 Cr in March"
            ui.label(theme::semibold("Adjustments").color(tk.muted));
            let (y0, m0) = self.chart.data.as_ref().and_then(|d| d.categories.last().and_then(|c| parse_month(c))).map(|(y, m)| if m == 12 { (y + 1, 1) } else { (y, m + 1) }).unwrap_or((2027, 1));
            let mut remove: Option<usize> = None;
            let names: Vec<String> = self.chart.data.as_ref().map(|d| d.kept_series.clone()).unwrap_or_default();
            for (i, o) in f.overlays.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt(("forecast_ov_m", i)).width(60.0).selected_text(MONTHS[(o.month.clamp(1, 12) - 1) as usize]).show_ui(ui, |ui| {
                        for (k, m) in MONTHS.iter().enumerate() {
                            ui.selectable_value(&mut o.month, k as u32 + 1, *m);
                        }
                    });
                    ui.add(egui::DragValue::new(&mut o.year).range(2000..=2100));
                    let mut cr = o.amount / CRORE;
                    if ui.add(egui::DragValue::new(&mut cr).speed(1.0).suffix(" Cr")).changed() {
                        o.amount = cr * CRORE;
                    }
                    ui.checkbox(&mut o.from_here, "and after");
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                f.overlays.remove(i);
            }
            if ui.button("Add an adjustment").on_hover_text("For example +200 Cr in March: a known inflow or outflow the history cannot show.").clicked() {
                f.overlays.push(OverlaySpec { year: y0, month: m0, amount: 0.0, from_here: false, series: String::new() });
            }
            let _ = names;
            if self.chart.spec.forecast.as_ref() != Some(&f) {
                self.chart.spec.forecast = Some(f);
            }
            ui.add_space(theme::SPACE_XS);
            if self.chart.busy {
                ui.label(egui::RichText::new("Calculating\u{2026}").color(tk.muted));
            }
            if let Some(e) = &self.chart.error {
                ui.add(egui::Label::new(egui::RichText::new(e).color(theme::danger(ui))).wrap());
            } else if let Some(d) = &self.chart.data {
                for n in d.notes.iter() {
                    ui.add(egui::Label::new(egui::RichText::new(n).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                }
            }
            ui.add(egui::Label::new(egui::RichText::new("To keep it: Update the chart tab (Chart tab, Update). Dashboards, PowerPoint, PDF, HTML and Excel then show the forecast.").size(theme::TEXT_LABEL).color(tk.muted)).wrap());
        });
    }
}
