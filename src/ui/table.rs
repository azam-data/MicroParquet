//! Screen module `table` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

impl MicroParquetApp {
    pub(crate) fn render_data_grid(
        ui: &mut egui::Ui,
        df: &DataFrame,
        col_formats: &HashMap<String, ColumnFormatSpec>,
        row_offset: usize,
        num_format: NumberDisplayFormat,
        global_decimals: usize,
    ) {
        let cols = df.get_columns();
        let num_rows = df.height();

        egui::ScrollArea::horizontal().show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(55.0).at_least(40.0));

            for _ in cols {
                builder = builder.column(Column::initial(145.0).at_least(80.0).clip(true));
            }

            builder
                .header(32.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("#");
                    });
                    for c in cols {
                        header.col(|ui| {
                            ui.strong(format!("{}\n({:?})", c.name(), c.dtype()));
                        });
                    }
                })
                .body(|body| {
                    body.rows(25.0, num_rows, |mut row| {
                        let r_idx = row.index();
                        row.col(|ui| {
                            ui.label((row_offset + r_idx + 1).to_string());
                        });
                        for c in cols {
                            row.col(|ui| {
                                let cname = c.name().as_str();
                                let spec = col_formats.get(cname);
                                let cell_text = match c.get(r_idx) {
                                    Ok(AnyValue::Null) => "null".to_string(),
                                    Ok(AnyValue::Float64(v)) => format_metric_value(
                                        v,
                                        cname,
                                        spec,
                                        num_format,
                                        global_decimals,
                                    ),
                                    Ok(AnyValue::Float32(v)) => format_metric_value(
                                        v as f64,
                                        cname,
                                        spec,
                                        num_format,
                                        global_decimals,
                                    ),
                                    Ok(AnyValue::String(s)) => s.to_string(),
                                    Ok(other) => format!("{}", other),
                                    Err(_) => "".to_string(),
                                };
                                ui.label(cell_text);
                            });
                        }
                    });
                });
        });
    }

    pub(crate) fn render_table_area(&mut self, ui: &mut egui::Ui) {
        let mut needs_refresh = false;
        let mut ctrl_col_changed = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Rows {} – {} of {}",
                    tab.viewport_offset + 1,
                    (tab.viewport_offset + tab.cached_slice.height()).min(tab.total_rows),
                    tab.total_rows
                ));

                if ui.button("⏮ First").clicked() && tab.viewport_offset > 0 {
                    tab.viewport_offset = 0;
                    needs_refresh = true;
                }
                if ui.button("◀ Prev 100").clicked() && tab.viewport_offset > 0 {
                    tab.viewport_offset =
                        tab.viewport_offset.saturating_sub(tab.viewport_limit);
                    needs_refresh = true;
                }
                if ui.button("Next 100 ▶").clicked()
                    && tab.viewport_offset + tab.viewport_limit < tab.total_rows
                {
                    tab.viewport_offset += tab.viewport_limit;
                    needs_refresh = true;
                }

                ui.separator();
                ui.label("Pinned Control Col:");
                let current_ctrl = tab
                    .control_col
                    .clone()
                    .unwrap_or_else(|| "None".to_string());
                egui::ComboBox::from_id_salt("ctrl_col_selector")
                    .selected_text(current_ctrl)
                    .show_ui(ui, |ui| {
                        for col_series in tab.cached_slice.get_columns() {
                            let cname = col_series.name().to_string();
                            if ui
                                .selectable_value(
                                    &mut tab.control_col,
                                    Some(cname.clone()),
                                    &cname,
                                )
                                .clicked()
                            {
                                needs_refresh = true;
                                ctrl_col_changed = true;
                            }
                        }
                    });
            });

            ui.separator();
            Self::render_data_grid(
                ui,
                &tab.cached_slice,
                &tab.col_formats,
                tab.viewport_offset,
                self.num_format,
                self.global_decimals,
            );
        }

        if needs_refresh {
            self.refresh_active_tab(ctrl_col_changed);
        }
    }
}
