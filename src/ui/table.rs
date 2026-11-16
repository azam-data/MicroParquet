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

    /// Filter chip bar (one chip per filter step) with the paging controls on the right.
    pub(crate) fn render_filter_bar(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let mut remove_step: Option<usize> = None;
        let mut open_filter = false;
        let mut page_prev = false;
        let mut page_next = false;
        let mut page_first = false;
        let mut ctrl_changed = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::semibold("Filters").size(theme::TEXT_SMALL).color(tk.muted));
                for (i, step) in tab.steps.iter().enumerate() {
                    if let TransformStep::FilterRows { column, operator, value, keep_matching } = step {
                        let mut text = format!("{} {} {}", column, operator.label(), value);
                        if !*keep_matching {
                            text = format!("not ({})", text.trim());
                        }
                        let (clicked, removed) = theme::chip(ui, text.trim());
                        if clicked {
                            open_filter = true;
                        }
                        if removed {
                            remove_step = Some(i);
                        }
                    }
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("+ Add condition").size(theme::TEXT_SMALL).color(tk.ink2))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::new(1.0_f32, tk.border))
                            .rounding(egui::Rounding::same(14.0)),
                    )
                    .clicked()
                {
                    open_filter = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_next = tab.viewport_offset + tab.viewport_limit < tab.total_rows;
                    if ui.add_enabled(can_next, egui::Button::new(theme::ph::CARET_RIGHT)).on_hover_text("Next page").clicked() {
                        page_next = true;
                    }
                    if ui.add_enabled(tab.viewport_offset > 0, egui::Button::new(theme::ph::CARET_LEFT)).on_hover_text("Previous page").clicked() {
                        page_prev = true;
                    }
                    if ui.add_enabled(tab.viewport_offset > 0, egui::Button::new(theme::ph::SKIP_BACK)).on_hover_text("First page").clicked() {
                        page_first = true;
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{}\u{2013}{} of {}",
                            format_with_commas((tab.slice_offset + 1) as f64, 0),
                            format_with_commas(
                                (tab.slice_offset + tab.cached_slice.height()).min(tab.total_rows) as f64,
                                0
                            ),
                            format_with_commas(tab.total_rows as f64, 0)
                        ))
                        .family(egui::FontFamily::Monospace)
                        .size(theme::TEXT_MONO)
                        .color(tk.ink),
                    );
                    ui.label(egui::RichText::new("Rows").color(tk.muted));
                    ui.separator();
                    let current_ctrl = tab.control_col.clone().unwrap_or_else(|| "None".to_string());
                    egui::ComboBox::from_id_salt("ctrl_col_selector")
                        .selected_text(current_ctrl)
                        .show_ui(ui, |ui| {
                            for col_series in tab.cached_slice.get_columns() {
                                let cname = col_series.name().to_string();
                                if ui.selectable_value(&mut tab.control_col, Some(cname.clone()), &cname).clicked() {
                                    ctrl_changed = true;
                                }
                            }
                        });
                    ui.label(egui::RichText::new("Control column").color(tk.muted));
                });
            });
        }
        let mut refresh = ctrl_changed;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            if page_first {
                tab.viewport_offset = 0;
                refresh = true;
            }
            if page_prev {
                tab.viewport_offset = tab.viewport_offset.saturating_sub(tab.viewport_limit);
                refresh = true;
            }
            if page_next && tab.viewport_offset + tab.viewport_limit < tab.total_rows {
                tab.viewport_offset += tab.viewport_limit;
                refresh = true;
            }
            if let Some(i) = remove_step {
                if i < tab.steps.len() {
                    tab.steps.remove(i);
                    tab.viewport_offset = 0;
                    refresh = true;
                }
            }
        }
        if open_filter {
            self.init_tool_defaults_from_active_tab();
            self.active_tool = ActiveToolModal::FilterRows;
        }
        if refresh {
            self.refresh_active_tab(ctrl_changed);
        }
        ui.add_space(theme::SPACE_M);
    }

    pub(crate) fn render_table_area(&mut self, ui: &mut egui::Ui) {
        // While a refresh runs, the last good page stays on screen, dimmed.
        let updating = self
            .tabs
            .get(self.active_tab)
            .is_some_and(|t| self.jobs.is_busy(JobKey::tab(JobKind::Refresh, t.id)));
        if let Some(tab) = self.tabs.get(self.active_tab) {
            ui.scope(|ui| {
                if updating {
                    ui.set_opacity(0.45);
                }
                Self::render_data_grid(
                    ui,
                    &tab.cached_slice,
                    &tab.col_formats,
                    tab.slice_offset,
                    self.num_format,
                    self.global_decimals,
                );
            });
        }
    }
}
