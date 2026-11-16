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
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_pivot_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        theme::modal("\u{E150}  Pivot & WAVG Builder (with Per-Metric Format & Decimal Precision)")
        .default_width(720.0)
        .show(ctx, |ui| {
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
                        .desired_width(130.0),
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

            ui.separator();
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "\u{E3FE} Generate Summary / Pivot View").clicked() {
                    let groups: Vec<String> = self
                        .pivot.group_cols
                        .iter()
                        .filter(|(_, c)| *c)
                        .map(|(n, _)| n.clone())
                        .collect();
                    let sort_c = self.pivot.aggs.first().map(|a| a.alias.clone());
                    step_to_apply = Some((
                        TransformStep::GroupByPivot {
                            group_cols: groups,
                            pivot_col: None,
                            aggs: self.pivot.aggs.clone(),
                            sort_by_col: sort_c,
                            top_n: if self.pivot.top_n > 0 {
                                Some(self.pivot.top_n)
                            } else {
                                None
                            },
                        },
                        self.pivot.open_in_new_tab,
                    ));
                    close_modal = true;
                }
                if ui.button("Cancel").clicked() {
                    close_modal = true;
                }
            });
        });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
