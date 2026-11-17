//! Screen module `chart` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct ChartState {
    pub x_col: String,
    pub y_col: String,
}

impl Default for ChartState {
    fn default() -> Self {
        Self {
            x_col: String::new(),
            y_col: String::new(),
        }
    }
}

impl MicroParquetApp {
    pub(crate) fn render_bar_chart(ui: &mut egui::Ui, df: &DataFrame, x_col: &str, y_col: &str) {
        let Ok(x_series) = df.column(x_col) else {
            return;
        };
        let Ok(y_series) = df.column(y_col) else {
            return;
        };

        let n = df.height().min(25);
        let mut bars = Vec::with_capacity(n);
        for i in 0..n {
            let label = match x_series.get(i) {
                Ok(AnyValue::String(s)) => s.to_string(),
                Ok(other) => format!("{}", other),
                Err(_) => format!("{}", i),
            };
            let val_cr = match y_series.get(i) {
                Ok(AnyValue::Float64(v)) => v / 10_000_000.0, // not a size: Crore
                Ok(AnyValue::Float32(v)) => (v as f64) / 10_000_000.0, // not a size: Crore
                Ok(AnyValue::Int64(v)) => (v as f64) / 10_000_000.0, // not a size: Crore
                Ok(AnyValue::UInt32(v)) => (v as f64) / 10_000_000.0, // not a size: Crore
                _ => 0.0,
            };
            bars.push(Bar::new(i as f64, val_cr).name(label));
        }

        Plot::new("alco_bar_plot")
            .height(theme::CHART_H)
            .show_axes([true, true])
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(BarChart::new(bars).width(theme::CHART_BAR_W));
            });
    }

    pub(crate) fn render_chart_drawer(&mut self, ui: &mut egui::Ui) {
        if self.show_chart_drawer {
            ui.separator();
            if let Some(tab) = self.tabs.get(self.active_tab) {
                let avail = tab.available_columns();
                ui.horizontal(|ui| {
                    ui.strong("ALCO Visual Bar Chart:");
                    ui.label("Category (X):");
                    egui::ComboBox::from_id_salt("chart_x")
                        .selected_text(&self.chart.x_col)
                        .show_ui(ui, |ui| {
                            for c in &avail {
                                ui.selectable_value(&mut self.chart.x_col, c.clone(), c);
                            }
                        });
                    ui.label("Value (Y - in BDT Crore):");
                    egui::ComboBox::from_id_salt("chart_y")
                        .selected_text(&self.chart.y_col)
                        .show_ui(ui, |ui| {
                            for c in &avail {
                                ui.selectable_value(&mut self.chart.y_col, c.clone(), c);
                            }
                        });
                });

                Self::render_bar_chart(
                    ui,
                    &tab.cached_slice,
                    &self.chart.x_col,
                    &self.chart.y_col,
                );
            }
        }
    }
}
