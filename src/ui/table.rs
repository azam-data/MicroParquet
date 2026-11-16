//! Screen module `table` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// Text widths measured with the fonts egui has loaded.
struct EguiMetrics<'a> {
    ctx: &'a egui::Context,
    /// Cells of this column are drawn in the mono font (numbers, dates, IDs).
    mono_cells: bool,
}

impl EguiMetrics<'_> {
    fn width(&self, text: &str, font: egui::FontId) -> f32 {
        self.ctx.fonts(|f| f.layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE).size().x)
    }
}

impl micro_parquet::table_layout::FontMetrics for EguiMetrics<'_> {
    fn header(&self, text: &str) -> f32 {
        self.width(text, egui::FontId::new(theme::TEXT_BASE, theme::family_semibold()))
    }
    fn selector(&self, text: &str) -> f32 {
        self.width(text, egui::FontId::new(theme::TEXT_LABEL, theme::family_semibold()))
    }
    fn summary(&self, text: &str) -> f32 {
        self.width(text, egui::FontId::new(theme::TEXT_MONO, theme::family_mono_semibold()))
    }
    fn cell(&self, text: &str) -> f32 {
        if self.mono_cells {
            self.width(text, egui::FontId::new(theme::TEXT_MONO, egui::FontFamily::Monospace))
        } else {
            self.width(text, egui::FontId::new(theme::TEXT_BASE, egui::FontFamily::Proportional))
        }
    }
}

/// How a column's cells are drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CellKind {
    /// Numbers: right-aligned, mono.
    Number,
    /// Dates and IDs: left-aligned, mono.
    MonoText,
    /// Everything else: left-aligned, Source Sans 3.
    Text,
}

fn cell_kind(c: &polars::prelude::Series) -> CellKind {
    let dt = c.dtype();
    if dt.is_numeric() {
        CellKind::Number
    } else if dt.is_temporal() || TabState::is_likely_id_column(c.name().as_str()) {
        CellKind::MonoText
    } else {
        CellKind::Text
    }
}

/// The text of one cell, as the grid shows it.
fn cell_text(
    c: &polars::prelude::Series,
    r_idx: usize,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
) -> String {
    let cname = c.name().as_str();
    let spec = col_formats.get(cname);
    match c.get(r_idx) {
        Ok(AnyValue::Null) => "null".to_string(),
        Ok(AnyValue::Float64(v)) => format_metric_value(v, cname, spec, num_format, global_decimals),
        Ok(AnyValue::Float32(v)) => format_metric_value(v as f64, cname, spec, num_format, global_decimals),
        Ok(AnyValue::String(s)) => s.to_string(),
        Ok(AnyValue::Int64(v)) if !TabState::is_likely_id_column(cname) => format_with_commas(v as f64, 0),
        Ok(other) => format!("{}", other),
        Err(_) => "".to_string(),
    }
}

impl MicroParquetApp {
    /// Draws the page. Returns the column whose header was clicked, if any.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_data_grid(
        ui: &mut egui::Ui,
        df: &DataFrame,
        col_formats: &HashMap<String, ColumnFormatSpec>,
        row_offset: usize,
        num_format: NumberDisplayFormat,
        global_decimals: usize,
        tab_id: u64,
        selected: Option<&str>,
    ) -> Option<String> {
        use micro_parquet::table_layout::{column_min_width, MAX_COLUMN_WIDTH, SAMPLE_ROWS};
        let tk = *theme::tokens(ui.visuals().dark_mode);
        let cols = df.get_columns();
        let num_rows = df.height();
        let ctx = ui.ctx().clone();
        let mut clicked: Option<String> = None;

        // Minimum widths: header, selector, summary and the first 100 rows' texts.
        let kinds: Vec<CellKind> = cols.iter().map(cell_kind).collect();
        let min_widths: Vec<f32> = cols
            .iter()
            .zip(&kinds)
            .map(|(c, kind)| {
                let m = EguiMetrics { ctx: &ctx, mono_cells: *kind != CellKind::Text };
                let samples: Vec<String> = (0..num_rows.min(SAMPLE_ROWS))
                    .map(|r| cell_text(c, r, col_formats, num_format, global_decimals))
                    .collect();
                column_min_width(c.name().as_str(), "", "", &samples, &m)
            })
            .collect();
        let row_no_width = EguiMetrics { ctx: &ctx, mono_cells: true };
        let row_no_min = {
            use micro_parquet::table_layout::FontMetrics;
            row_no_width.cell(&format_with_commas((row_offset + num_rows.max(1)) as f64, 0)) + 24.0
        };

        egui::Frame::none()
            .fill(tk.surface)
            .stroke(egui::Stroke::new(1.0_f32, tk.border))
            .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    // The id keeps each tab's dragged column widths apart.
                    let mut builder = TableBuilder::new(ui)
                        .id_salt(("data_grid", tab_id))
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::initial(row_no_min.max(48.0)).at_least(row_no_min.max(40.0)));
                    for w in &min_widths {
                        builder = builder.column(
                            Column::initial((*w).max(96.0).min(MAX_COLUMN_WIDTH))
                                .at_least(*w)
                                .clip(true)
                                .resizable(true),
                        );
                    }
                    builder
                        .header(theme::HEADER_ROW_H, |mut header| {
                            header.col(|ui| {
                                ui.label(theme::label_text("#").color(tk.muted));
                            });
                            for (c, kind) in cols.iter().zip(&kinds) {
                                header.col(|ui| {
                                    let rect = ui.max_rect();
                                    let name = c.name().to_string();
                                    let is_sel = selected == Some(name.as_str());
                                    let resp = ui.interact(rect, ui.id().with(("hdr", name.as_str())), egui::Sense::click());
                                    if is_sel {
                                        ui.painter().rect_filled(rect, 0.0, tk.accent_soft);
                                        ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, egui::Stroke::new(2.0_f32, tk.accent));
                                    }
                                    let layout = if *kind == CellKind::Number {
                                        egui::Layout::right_to_left(egui::Align::Center)
                                    } else {
                                        egui::Layout::left_to_right(egui::Align::Center)
                                    };
                                    ui.with_layout(layout, |ui| {
                                        ui.add(egui::Label::new(theme::semibold(name.clone()).color(tk.ink)).truncate());
                                    });
                                    resp.clone().on_hover_text(format!("{}  \u{b7}  {:?}", name, c.dtype()));
                                    if resp.clicked() {
                                        clicked = Some(name);
                                    }
                                });
                            }
                        })
                        .body(|body| {
                            body.rows(theme::DATA_ROW_H, num_rows, |mut row| {
                                let r_idx = row.index();
                                row.col(|ui| {
                                    ui.label(theme::mono(format_with_commas((row_offset + r_idx + 1) as f64, 0)).color(tk.muted));
                                });
                                for (c, kind) in cols.iter().zip(&kinds) {
                                    row.col(|ui| {
                                        let text = cell_text(c, r_idx, col_formats, num_format, global_decimals);
                                        let is_null = text == "null";
                                        let rich = match kind {
                                            CellKind::Text => egui::RichText::new(text.clone()),
                                            _ => theme::mono(text.clone()),
                                        }
                                        .color(if is_null { tk.muted } else { tk.ink });
                                        let layout = if *kind == CellKind::Number {
                                            egui::Layout::right_to_left(egui::Align::Center)
                                        } else {
                                            egui::Layout::left_to_right(egui::Align::Center)
                                        };
                                        ui.with_layout(layout, |ui| {
                                            let r = ui.add(egui::Label::new(rich).truncate());
                                            // A cut cell shows its full text on hover.
                                            let cut = text.chars().count() > 6
                                                && r.rect.width() >= ui.max_rect().width() - 0.5;
                                            if cut {
                                                r.on_hover_text(text.clone());
                                            }
                                        });
                                    });
                                }
                            });
                        });
                });
            });
        clicked
    }

    /// Filter chip bar (one chip per filter step) with the paging controls on the right.
    pub(crate) fn render_filter_bar(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let mut remove_cond: Option<(usize, usize)> = None;
        let mut edit_step: Option<usize> = None;
        let mut add_condition = false;
        let mut page_prev = false;
        let mut page_next = false;
        let mut page_first = false;
        let mut ctrl_changed = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::semibold("Filters").size(theme::TEXT_SMALL).color(tk.muted));
                for (i, step) in tab.steps.iter().enumerate() {
                    let Some((conds, join, keep)) = step.as_filter_group() else {
                        continue;
                    };
                    for (j, c) in conds.iter().enumerate() {
                        if j > 0 {
                            ui.label(egui::RichText::new(join.label()).size(theme::TEXT_SMALL).color(tk.muted));
                        }
                        let text = if keep { c.describe() } else { format!("delete: {}", c.describe()) };
                        let (clicked, removed) = theme::chip(ui, &text);
                        if clicked {
                            edit_step = Some(i);
                        }
                        if removed {
                            remove_cond = Some((i, j));
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
                    add_condition = true;
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
                        egui::RichText::new(micro_parquet::table_layout::format_row_range(
                            tab.slice_offset + 1,
                            (tab.slice_offset + tab.cached_slice.height()).min(tab.total_rows),
                            tab.total_rows,
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
        }
        if let Some((i, j)) = remove_cond {
            self.remove_filter_condition(i, j);
        }
        if let Some(i) = edit_step {
            self.open_filter_editor(Some(i), false);
        } else if add_condition {
            self.open_filter_editor_to_add();
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
        let mut clicked: Option<String> = None;
        if let Some(tab) = self.tabs.get(self.active_tab) {
            ui.scope(|ui| {
                if updating {
                    ui.set_opacity(0.45);
                }
                clicked = Self::render_data_grid(
                    ui,
                    &tab.cached_slice,
                    &tab.col_formats,
                    tab.slice_offset,
                    self.num_format,
                    self.global_decimals,
                    tab.id,
                    self.selected_column.as_deref(),
                );
            });
        }
        if let Some(c) = clicked {
            // Clicking a header selects the column and opens the Column tab.
            self.selected_column = Some(c);
            self.right_tab = RightTab::Column;
            self.show_steps_sidebar = true;
        }
    }
}
