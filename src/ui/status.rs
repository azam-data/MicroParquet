//! Screen module `status` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

impl MicroParquetApp {
    pub(crate) fn render_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_reconciliation_bar").show(ctx, |ui| {
            let mut reset_baseline_clicked = false;
            ui.horizontal(|ui| {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    ui.strong(format!(
                        "Rows: {}",
                        tab.totals
                            .rows
                            .map(|r| format_with_commas(r as f64, 0))
                            .unwrap_or_else(|| "—".to_string())
                    ));
                    ui.separator();
                    if let (Some(col_name), None) = (&tab.control_col, tab.control_sum) {
                        ui.colored_label(
                            theme::WARN_ORANGE,
                            format!("Control [{}]: —", col_name),
                        );
                        ui.separator();
                    }
                    if let Some(note) = &tab.totals.note {
                        ui.colored_label(theme::WARN_ORANGE, format!("⚠ {}", note));
                        ui.separator();
                    }

                    if let (Some(col_name), Some(sum_val)) = (&tab.control_col, tab.control_sum) {
                        ui.colored_label(
                            theme::INFO_BLUE,
                            format!(
                                "Control [{}]: {:.2} Cr (Raw: {})",
                                col_name,
                                sum_val / 10_000_000.0,
                                format_with_commas(sum_val, 2)
                            ),
                        );
                        if let Some(base) = tab.baseline_control_sum {
                            let diff = sum_val - base;
                            if diff.abs() > 0.01 {
                                ui.colored_label(
                                    theme::WARN_ORANGE,
                                    format!("Δ vs Source: {:+.2} Cr", diff / 10_000_000.0),
                                );
                                if ui
                                    .small_button("🔄 Reset Baseline")
                                    .on_hover_text("Pin current total as the new 0.00 Cr baseline")
                                    .clicked()
                                {
                                    reset_baseline_clicked = true;
                                }
                            }
                        }
                        ui.separator();
                    }
                }
                if let Some(rc) = self.tabs.get(self.active_tab).and_then(|t| t.row_check.as_ref()) {
                    if rc.warning().is_some() {
                        ui.colored_label(
                            theme::WARN_ORANGE,
                            format!(
                                "⚠ Rows read {} vs file data lines {}",
                                format_with_commas(rc.parsed_rows as f64, 0),
                                format_with_commas(rc.data_lines() as f64, 0)
                            ),
                        )
                        .on_hover_text(rc.warning().unwrap_or_default());
                        ui.separator();
                    }
                }
                if let Some(js) = self.tabs.get(self.active_tab).and_then(|t| t.join_status.as_ref()) {
                    let warn = js.duplicate_keys > 0 || js.rows != js.left_rows;
                    let color = if warn {
                        theme::WARN_ORANGE
                    } else {
                        theme::INFO_BLUE
                    };
                    ui.colored_label(color, js.summary());
                    ui.separator();
                }
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    if !tab.parse_failures.is_empty() {
                        let detail: Vec<String> = tab
                            .parse_failures
                            .iter()
                            .map(|f| format!("{}: {}", f.column, f.examples.join(" | ")))
                            .collect();
                        ui.colored_label(
                            theme::WARN_ORANGE,
                            format!("⚠ {}", parse_failure_summary(&tab.parse_failures)),
                        )
                        .on_hover_text(format!(
                            "Non-blank values that could not be converted and are null. Examples:\n{}",
                            detail.join("\n")
                        ));
                        ui.separator();
                    }
                }
                if let Some(job) = self.jobs.active_long() {
                    ui.spinner();
                    let text = if job.cancelling {
                        format!("Cancelling {}…", job.label)
                    } else if job.progress.text.is_empty() {
                        format!("{}…", job.label)
                    } else {
                        job.progress.text.clone()
                    };
                    ui.label(text);
                    if let Some(f) = job.progress.fraction {
                        ui.add(egui::ProgressBar::new(f.clamp(0.0, 1.0)).desired_width(120.0));
                    }
                    if ui.add_enabled(!job.cancelling, egui::Button::new("Cancel")).clicked() {
                        self.jobs.cancel(job.id);
                    }
                    ui.separator();
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                let updating = self.tabs.get(self.active_tab).is_some_and(|t| {
                    self.jobs.is_busy(JobKey::tab(JobKind::Refresh, t.id))
                        || self.jobs.is_busy(JobKey::tab(JobKind::Schema, t.id))
                });
                if updating {
                    ui.colored_label(theme::INFO_BLUE, "Updating…");
                    ui.separator();
                }
                ui.label(&self.status_message);
            });

            if reset_baseline_clicked {
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.baseline_control_sum = tab.control_sum;
                }
            }
        });
    }
}
