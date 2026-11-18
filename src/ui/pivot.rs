//! Screen module `pivot` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct PivotState {
    pub group_cols: Vec<(String, bool)>,
    pub aggs: Vec<AggSpec>,
    pub new_target: String,
    pub new_func: AggFunc,
    pub new_weight: String,
    pub new_alias: String,
    pub new_fmt: ColumnFormatSpec,
    pub top_n: usize,
    pub open_in_new_tab: bool,
    /// The conversion to the new pivot waiting for the user's confirmation (Phase 4 item 1).
    pub convert: Option<micro_parquet::pivot_table::PivotConversion>,
    /// Why the step cannot be converted, shown under the button.
    pub convert_error: Option<String>,
}

impl Default for PivotState {
    fn default() -> Self {
        Self {
            group_cols: Vec::new(),
            aggs: Vec::new(),
            new_target: "Balance".to_string(),
            new_func: AggFunc::Sum,
            new_weight: "Balance".to_string(),
            new_alias: "Total_Balance".to_string(),
            new_fmt: ColumnFormatSpec {
                        fmt_type: MetricFormatType::Auto,
                        decimals: 2,
                    },
            top_n: 0,
            open_in_new_tab: true,
            convert: None,
            convert_error: None,
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_pivot_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        let editing_this = self.editing.as_ref().is_some_and(|e| e.tool == ActiveToolModal::GroupByPivot);
        theme::modal("\u{E150}  Pivot & WAVG Builder (with Per-Metric Format & Decimal Precision)")
        .default_width(theme::DIALOG_W_PIVOT)
        .show(ctx, |d| {
            d.body(|ui| {
                ui.strong("1. Select Row Grouping Columns:");
                ui.horizontal_wrapped(|ui| {
                    for (cname, checked) in &mut self.pivot.group_cols {
                        ui.checkbox(checked, cname.as_str());
                    }
                });

                ui.separator();
                ui.strong("2. Configure Value Metric & Display Format (Crore vs % vs Decimals):");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("piv_func")
                        .selected_text(self.pivot.new_func.label())
                        .show_ui(ui, |ui| {
                            for f in AggFunc::all() {
                                ui.selectable_value(&mut self.pivot.new_func, *f, f.label());
                            }
                        });
                    ui.label("Target:");
                    egui::ComboBox::from_id_salt("piv_tgt")
                        .selected_text(&self.pivot.new_target)
                        .show_ui(ui, |ui| {
                            for c in &avail_cols {
                                ui.selectable_value(&mut self.pivot.new_target, c.clone(), c);
                            }
                        });
                    if self.pivot.new_func == AggFunc::WeightedAvg {
                        ui.label("Weight:");
                        egui::ComboBox::from_id_salt("piv_wgt")
                            .selected_text(&self.pivot.new_weight)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(
                                        &mut self.pivot.new_weight,
                                        c.clone(),
                                        c,
                                    );
                                }
                            });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Alias:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pivot.new_alias)
                            .desired_width(theme::FIELD_W_VALUE),
                    );
                    ui.label("Format:");
                    egui::ComboBox::from_id_salt("piv_fmt")
                        .selected_text(self.pivot.new_fmt.fmt_type.label())
                        .show_ui(ui, |ui| {
                            for ft in MetricFormatType::all() {
                                ui.selectable_value(
                                    &mut self.pivot.new_fmt.fmt_type,
                                    *ft,
                                    ft.label(),
                                );
                            }
                        });
                    ui.label("Decimals:");
                    ui.add(
                        egui::DragValue::new(&mut self.pivot.new_fmt.decimals).range(0..=6),
                    );
                    if ui.button("\u{E3D4} Add Metric").clicked() {
                        self.pivot.aggs.push(AggSpec {
                            target_col: self.pivot.new_target.clone(),
                            func: self.pivot.new_func,
                            weight_col: if self.pivot.new_func == AggFunc::WeightedAvg {
                                Some(self.pivot.new_weight.clone())
                            } else {
                                None
                            },
                            alias: self.pivot.new_alias.clone(),
                            format_spec: self.pivot.new_fmt,
                        });
                    }
                });

                for a in &self.pivot.aggs {
                    ui.label(format!(
                        "  • {}: {:?}({}) — Format: {} ({} dec)",
                        a.alias,
                        a.func,
                        a.target_col,
                        a.format_spec.fmt_type.label(),
                        a.format_spec.decimals
                    ));
                }
                if !self.pivot.aggs.is_empty() && ui.small_button("Clear Metrics").clicked() {
                    self.pivot.aggs.clear();
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Top-N Limit (0 = All Rows):");
                    ui.add(egui::DragValue::new(&mut self.pivot.top_n).range(0..=10000));
                    ui.checkbox(&mut self.pivot.open_in_new_tab, "Open Pivot in New Tab");
                });

            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if theme::primary_button(ui, "\u{E3FE} Generate Summary / Pivot View").clicked() {
                        step_to_apply = self.tool_step(&ActiveToolModal::GroupByPivot).map(|s| (s, self.pivot.open_in_new_tab));
                        close_modal = true;
                    }
                    if editing_this {
                        if ui
                            .button("Convert to new pivot\u{2026}")
                            .on_hover_text("Turns this step into a pivot-builder step. You see the column names that change before anything is converted.")
                            .clicked()
                        {
                            match self.tool_step(&ActiveToolModal::GroupByPivot).map(|s| micro_parquet::pivot_table::convert_group_by_pivot(&s)) {
                                Some(Ok(c)) => {
                                    self.pivot.convert = Some(c);
                                    self.pivot.convert_error = None;
                                }
                                Some(Err(e)) => self.pivot.convert_error = Some(format!("{e:#}")),
                                None => {}
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
                if let Some(e) = &self.pivot.convert_error {
                    ui.colored_label(theme::tokens(self.dark_mode).danger, e);
                }
            });
        });
        if self.pivot.convert.is_some() {
            if let Some(done) = self.render_pivot_convert_confirm(ctx) {
                if done {
                    close_modal = true;
                }
            }
        }
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}

impl MicroParquetApp {
    /// The confirmation of "Convert to new pivot": lists the output columns whose names change and
    /// what else changes. `Some(true)` when the step was converted (close the older dialog),
    /// `Some(false)` when the user went back.
    pub(crate) fn render_pivot_convert_confirm(&mut self, ctx: &egui::Context) -> Option<bool> {
        let conv = self.pivot.convert.clone()?;
        let mut result: Option<bool> = None;
        theme::modal("Convert to new pivot").default_width(theme::DIALOG_W_PIVOT_CONVERT).show(ctx, |d| {
            d.body(|ui| {
                if conv.renames.is_empty() {
                    ui.label("No column name changes.");
                } else {
                    ui.strong("These column names will change:");
                    egui::Grid::new("pivot_convert_renames").num_columns(3).striped(true).show(ui, |ui| {
                        for (old, new) in &conv.renames {
                            ui.label(old);
                            ui.label("\u{2192}");
                            ui.label(new);
                            ui.end_row();
                        }
                    });
                    ui.label("Anything that refers to the old names (later steps, saved macros) must be changed to the new ones.");
                }
                if !conv.notes.is_empty() {
                    ui.add_space(theme::SPACE_S);
                    ui.strong("Also:");
                    for n in &conv.notes {
                        ui.label(format!("\u{2022} {n}"));
                    }
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if theme::primary_button(ui, "Convert").clicked() {
                        result = Some(true);
                    }
                    if ui.button("Back").clicked() {
                        result = Some(false);
                    }
                });
            });
        });
        match result {
            Some(true) => {
                self.apply_pivot_conversion(conv);
                self.pivot.convert = None;
                Some(true)
            }
            Some(false) => {
                self.pivot.convert = None;
                Some(false)
            }
            None => None,
        }
    }

    /// Replaces the step being edited by the new pivot step, in place (the tab's other steps stay).
    pub(crate) fn apply_pivot_conversion(&mut self, conv: micro_parquet::pivot_table::PivotConversion) {
        let Some(ed) = self.editing.clone() else {
            return;
        };
        let Some(pos) = self.tabs.iter().position(|t| t.id == ed.tab_id) else {
            self.status_message = "The tab of the edited step was closed; nothing was changed.".to_string();
            return;
        };
        self.active_tab = pos;
        let was_off = self.tabs.get(pos).and_then(|t| t.steps.get(ed.index)).is_some_and(|s| s.is_skipped());
        let mut step = TransformStep::from_pivot_spec(conv.spec);
        if was_off {
            step = TransformStep::Skipped { step: Box::new(step) };
        }
        let status = format!("Step {} converted to the new pivot.", ed.index + 1);
        self.replace_step_in_active_tab(ed.index, step, &status);
        self.selected_step = Some(ed.index);
    }
}
