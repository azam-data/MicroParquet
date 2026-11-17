//! Screen module `status` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// What the status bar drew in its last frame (tests read it).
#[derive(Clone, Copy, Debug)]
pub struct StatusReport {
    /// The bar's content area.
    pub bar: egui::Rect,
    /// What the items and the message may use, left of the Messages button.
    pub left: egui::Rect,
    /// What they used.
    pub content: egui::Rect,
    /// The Messages button.
    pub messages: egui::Rect,
}

/// The status bar's last frame.
pub fn status_report(ctx: &egui::Context) -> Option<StatusReport> {
    ctx.data(|d| d.get_temp::<StatusReport>(egui::Id::new("status_report")))
}

/// How many messages "Messages" keeps.
pub const MESSAGE_LOG_LIMIT: usize = 50;

/// 1536 -> "1.5 KB", 31_000_000 -> "29.6 MB".
pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() { // not a size: bytes per KB
        v /= 1024.0; // not a size: bytes per KB
        u += 1;
    }
    if u == 0 {
        format!("{} B", n)
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

impl MicroParquetApp {
    /// Status bar (32 px): rows, control total, change against the source file, parse failures,
    /// scale, and the job progress or last message.
    pub(crate) fn render_status_bar(&mut self, ctx: &egui::Context) {
        let tk = *theme::tokens(self.dark_mode);
        let mut reset_baseline_clicked = false;
        let mut open_row_details = false;
        let mut open_messages = false;
        let mut status_content = egui::Rect::NOTHING;
        let mut status_bar = egui::Rect::NOTHING;
        let mut status_left = egui::Rect::NOTHING;
        let mut status_button = egui::Rect::NOTHING;
        self.log_status_message();
        let mut cancel_job: Option<u64> = None;
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(theme::STATUS_BAR_H)
            .frame(egui::Frame::none().fill(tk.surface).inner_margin(theme::MARGIN_STATUS_BAR))
            .show(ctx, |ui| {
                theme::top_border(ui, tk.border);
                // The "Messages" button sits at the far right; everything else, ending with the
                // last message, shares the room to its left and is cut with "…" or clipped there,
                // never drawn past the window.
                let full = ui.max_rect();
                let btn_w = theme::status_button_width(ui, "Messages");
                let right_rect = egui::Rect::from_min_max(egui::pos2(full.right() - btn_w, full.top()), full.max);
                let left_rect = egui::Rect::from_min_max(full.min, egui::pos2(right_rect.left() - theme::STATUS_ITEM_GAP, full.bottom()));
                let bar_w = left_rect.width();
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(left_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                    ui.set_clip_rect(left_rect.intersect(ui.clip_rect()));
                    ui.spacing_mut().item_spacing.x = theme::STATUS_ITEM_GAP;
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                    // an optional item is drawn only when the last message keeps some room
                    // (a running job keeps the room its bar, time and Cancel button need)
                    let job_reserve = if self.jobs.active_long().is_some() { theme::STATUS_JOB_RESERVE } else { 0.0 };
                    let room_for = |ui: &egui::Ui, w: f32| ui.available_width() - w - theme::STATUS_ITEM_GAP - job_reserve >= theme::STATUS_MESSAGE_ROOM;
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        let count = |n: Option<usize>| {
                            n.map(|r| format_with_commas(r as f64, 0)).unwrap_or_else(|| "\u{2014}".to_string())
                        };
                        ui.label(egui::RichText::new("Rows").color(tk.muted));
                        ui.label(theme::mono_semibold(count(tab.totals.rows)));
                        if let (Some(col_name), None) = (&tab.control_col, tab.control_sum) {
                            theme::warning_label(ui, &format!("Control [{}]: \u{2014}", col_name));
                        }
                        if let Some(note) = &tab.totals.note {
                            theme::warning_label(ui, note);
                        }
                        if let (Some(col_name), Some(sum_val)) = (&tab.control_col, tab.control_sum) {
                            theme::clipped_label(ui, egui::RichText::new(format!("Control [{}]", col_name)).color(tk.muted), bar_w * theme::STATUS_LABEL_SHARE)
                                .on_hover_text(col_name.clone());
                            ui.label(theme::mono_semibold(format!("{:.2} Cr", sum_val / 10_000_000.0))) // not a size: Crore
                                .on_hover_text(format!("Raw: {}", format_with_commas(sum_val, 2)));
                            if let Some(base) = tab.baseline_control_sum.filter(|_| room_for(ui, 2.0 * theme::STATUS_ITEM_GAP + theme::text_width(ui, "\u{394} vs source +000.00 Cr", theme::TEXT_SMALL))) {
                                let diff = sum_val - base;
                                ui.label(egui::RichText::new("\u{394} vs source").color(tk.muted));
                                if diff.abs() > 0.01 { // not a size: a difference below a paisa is none
                                    ui.label(
                                        theme::mono_semibold(format!("{:+.2} Cr", diff / 10_000_000.0)) // not a size: Crore
                                            .color(tk.warning),
                                    );
                                    if ui
                                        .small_button(theme::ph::ARROWS_CLOCKWISE)
                                        .on_hover_text("Pin the current total as the new baseline (0.00 Cr)") // not a size: text
                                        .clicked()
                                    {
                                        reset_baseline_clicked = true;
                                    }
                                } else {
                                    egui::Frame::none()
                                        .fill(tk.success_soft)
                                        .rounding(egui::Rounding::same(theme::RADIUS_SMALL))
                                        .inner_margin(theme::MARGIN_CHIP_ZERO)
                                        .show(ui, |ui| {
                                            ui.label(theme::mono_semibold("0.00 Cr").color(tk.success)); // not a size: text
                                        });
                                }
                            }
                        }
                        if let Some(rc) = tab.row_check.as_ref() {
                            // rows read against the file's lines: grey when line breaks inside
                            // quotes explain every missing line, a warning otherwise
                            let note = rc.status_text().map(|t| theme::icons_in_text(&t));
                            // (its width: the text up to its cap, and the Details button)
                            let note_w = note.as_ref().map(|t| theme::text_width(ui, t, theme::TEXT_SMALL).min(bar_w * theme::STATUS_LABEL_SHARE) + theme::status_button_width(ui, "Details") + theme::STATUS_ITEM_GAP);
                            if let Some(text) = note.filter(|_| note_w.is_some_and(|w| room_for(ui, w))) {
                                let color = if rc.is_explained() { tk.muted } else { tk.warning };
                                theme::clipped_label(ui, egui::RichText::new(text).color(color), bar_w * theme::STATUS_LABEL_SHARE)
                                    .on_hover_text(rc.warning().unwrap_or_default());
                                if ui.small_button("Details").on_hover_text("Which lines differ").clicked() {
                                    open_row_details = true;
                                }
                            }
                        }
                        // Stays while the page is stale, whatever message came after it.
                        if let Some(e) = &tab.page_error {
                            ui.label(
                                theme::semibold(format!("{}  Page out of date", theme::ph::WARNING)).color(tk.warning),
                            )
                            .on_hover_text(crate::ui::jobs::page_error_message(e));
                        }
                        if let Some(js) = tab.join_status.as_ref().filter(|js| room_for(ui, theme::text_width(ui, &js.summary(), theme::TEXT_SMALL).min(bar_w * theme::STATUS_LABEL_SHARE))) {
                            let warn = js.duplicate_keys > 0 || js.rows != js.left_rows;
                            theme::clipped_label(
                                ui,
                                egui::RichText::new(js.summary()).color(if warn { tk.warning } else { tk.accent }),
                                bar_w * theme::STATUS_LABEL_SHARE,
                            )
                            .on_hover_text(js.summary());
                        }
                        // Parse-failure chip: green at zero, warning colour otherwise.
                        let chip_text = if tab.parse_failures.is_empty() { "0 parse failures".to_string() } else { format!("\u{26A0} {}", parse_failure_summary(&tab.parse_failures)) };
                        let chip_w = theme::text_width(ui, &chip_text, theme::TEXT_SMALL).min(bar_w * theme::STATUS_LABEL_SHARE) + 2.0 * theme::SPACE_M;
                        if !room_for(ui, chip_w) {
                        } else if tab.parse_failures.is_empty() {
                            egui::Frame::none()
                                .fill(tk.success_soft)
                                .rounding(egui::Rounding::same(theme::RADIUS_PILL))
                                .inner_margin(theme::MARGIN_CHIP_PILL)
                                .show(ui, |ui| {
                                    ui.label(theme::semibold("0 parse failures").color(tk.success));
                                });
                        } else {
                            let detail: Vec<String> = tab
                                .parse_failures
                                .iter()
                                .map(|f| format!("{}: {}", f.column, f.examples.join(" | ")))
                                .collect();
                            theme::clipped_label(
                                ui,
                                theme::semibold(theme::icons_in_text(&format!(
                                    "\u{26A0} {}",
                                    parse_failure_summary(&tab.parse_failures)
                                )))
                                .color(tk.warning),
                                bar_w * theme::STATUS_LABEL_SHARE,
                            )
                            .on_hover_text(format!(
                                "Non-blank values that could not be converted and are null. Examples:\n{}",
                                detail.join("\n")
                            ));
                        }
                        let scale = format!("Scale: {}", self.num_format.label());
                        let scale_w = theme::text_width(ui, &scale, theme::TEXT_SMALL).min(bar_w * theme::STATUS_LABEL_SHARE);
                        if room_for(ui, scale_w) {
                        theme::clipped_label(ui, egui::RichText::new(scale.clone()).color(tk.muted), (ui.available_width() - theme::STATUS_MIN_MESSAGE_W).max(0.0).min(bar_w * theme::STATUS_LABEL_SHARE))
                            .on_hover_text(scale);
                        }
                    }
                    if let Some(job) = self.jobs.active_long() {
                        ui.spinner();
                        // what the job line needs, so that it can shrink before it pushes anything
                        // out of the bar: all stages, then the current stage only, then nothing
                        let small = theme::TEXT_SMALL;
                        let labels = job.stage_labels();
                        let cancel_w = theme::status_button_width(ui, "Cancel");
                        let tail_w = if job.progress.fraction.is_some() { theme::PROGRESS_BAR_W + theme::STATUS_ITEM_GAP } else { 0.0 }
                            + theme::text_width(ui, &micro_parquet::jobs::format_elapsed(job.elapsed()), small)
                            + cancel_w
                            + 2.0 * theme::STATUS_ITEM_GAP;
                        let all_stages_w: f32 = labels.iter().map(|l| theme::text_width(ui, l, small) + 2.0 * theme::STATUS_ITEM_GAP).sum::<f32>()
                            + if job.progress.bytes.is_some() { theme::text_width(ui, &job.progress.text, small) + theme::STATUS_ITEM_GAP } else { 0.0 };
                        let room = ui.available_width() - theme::STATUS_MESSAGE_ROOM - theme::STATUS_JOB_SLACK;
                        let show_all = job.progress.stages.len() > 1 && all_stages_w + tail_w <= room;
                        let show_text = all_stages_w + tail_w <= room || tail_w + theme::STATUS_MIN_MESSAGE_W + theme::STATUS_JOB_SLACK <= ui.available_width();
                        if job.cancelling {
                            ui.label(format!("Cancelling {}\u{2026}", job.label));
                        } else if show_all {
                            // "Reading 1/3 · Parsing 2/3 · Checking 3/3": the current stage stands out
                            for (i, lab) in labels.iter().enumerate() {
                                if i == job.progress.stage {
                                    ui.label(theme::semibold(lab.clone()).color(tk.ink));
                                } else {
                                    ui.label(egui::RichText::new(lab.clone()).color(tk.muted));
                                }
                                if i + 1 < labels.len() {
                                    ui.label(egui::RichText::new("\u{b7}").color(tk.muted));
                                }
                            }
                            // only a loop that reads the file itself has something more to say
                            if job.progress.bytes.is_some() {
                                ui.label(egui::RichText::new(job.progress.text.clone()).color(tk.accent));
                            }
                        } else if show_text {
                            // not enough room for every stage: the current one, the rest on hover
                            let now = labels.get(job.progress.stage).cloned().unwrap_or_else(|| {
                                if job.progress.text.is_empty() { format!("{}\u{2026}", job.label) } else { job.progress.text.clone() }
                            });
                            ui.label(theme::semibold(now)).on_hover_text(labels.join("  \u{b7}  "));
                        }
                        if let Some(f) = job.progress.fraction {
                            let mut bar = egui::ProgressBar::new(f.clamp(0.0, 1.0)).desired_width(theme::PROGRESS_BAR_W);
                            // a number is written only for bytes really read
                            if let (Some(pct), Some((read, total))) = (job.percent(), job.progress.bytes) {
                                bar = bar.text(format!("{}%", pct));
                                ui.add(bar).on_hover_text(format!("{} of {} read", format_bytes(read), format_bytes(total)));
                            } else {
                                ui.add(bar);
                            }
                        }
                        ui.label(theme::mono(micro_parquet::jobs::format_elapsed(job.elapsed())).color(tk.muted));
                        if ui.add_enabled(!job.cancelling, egui::Button::new("Cancel")).clicked() {
                            cancel_job = Some(job.id);
                        }
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                    }
                    let updating = self.tabs.get(self.active_tab).is_some_and(|t| {
                        self.jobs.is_busy(JobKey::tab(JobKind::Refresh, t.id))
                            || self.jobs.is_busy(JobKey::tab(JobKind::Schema, t.id))
                    });
                    if updating {
                        ui.label(theme::semibold("Updating\u{2026}").color(tk.accent));
                    }
                    // the last message: cut with "…" when it does not fit, the whole text on hover
                    let text = theme::icons_in_text(&self.status_message);
                    let room = ui.available_width().max(0.0);
                    if room > theme::STATUS_MIN_MESSAGE_W {
                        let natural = theme::text_width(ui, &text, theme::TEXT_SMALL);
                        let r = ui.add(egui::Label::new(text.clone()).truncate());
                        if natural > room {
                            r.on_hover_text(text);
                        }
                    }
                    status_content = ui.min_rect();
                    },
                );
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(right_rect).layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                        let b = ui.small_button("Messages").on_hover_text("The last 50 messages");
                        status_button = b.rect;
                        if b.clicked() {
                            open_messages = true;
                        }
                    },
                );
                status_bar = full;
                status_left = left_rect;
            });
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("status_report"), StatusReport { bar: status_bar, left: status_left, content: status_content, messages: status_button }));
        if open_messages {
            self.messages_open = true;
        }
        if let Some(id) = cancel_job {
            self.jobs.cancel(id);
        }
        if open_row_details {
            self.row_details_open = true;
        }
        if reset_baseline_clicked {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.baseline_control_sum = tab.control_sum;
            }
        }
    }

    /// "Details": up to 20 physical lines where a record spans several lines (a quoted line break)
    /// or has the wrong number of fields, with the first 80 characters of each.
    pub(crate) fn render_row_details(&mut self, ctx: &egui::Context) {
        if !self.row_details_open {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let Some(rc) = self.tabs.get(self.active_tab).and_then(|t| t.row_check.clone()) else {
            self.row_details_open = false;
            return;
        };
        let mut close = false;
        theme::modal("Row count details").default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                ui.label(
                    rc.status_text()
                        .map(|t| theme::icons_in_text(&t))
                        .unwrap_or_default(),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} records contain line breaks inside quotes ({} extra lines); {} records have another number of fields than the header.",
                        format_with_commas(rc.multiline_records as f64, 0),
                        format_with_commas(rc.explained_lines as f64, 0),
                        format_with_commas(rc.malformed_records as f64, 0)
                    ))
                    .color(tk.muted),
                );
                ui.add_space(theme::SPACE_S);
                if rc.details.is_empty() {
                    ui.label("No single line stands out. Check the separator, title lines and encoding.");
                }
                for e in &rc.details {
                    let what = match e.issue {
                        micro_parquet::models::LineIssue::QuotedLineBreak => format!("record over {} lines (line break inside quotes)", e.span),
                        micro_parquet::models::LineIssue::WrongFieldCount => format!("did not parse: {} fields, the header has {}", e.fields, e.expected_fields),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(theme::mono_semibold(format!("Line {}", format_with_commas(e.line as f64, 0))));
                        ui.label(egui::RichText::new(what).color(tk.muted));
                    });
                    ui.label(theme::mono(e.text.clone()));
                    ui.add_space(theme::SPACE_XS);
                }
                let shown = rc.details.len();
                let total = rc.multiline_records + rc.malformed_records;
                if total > shown {
                    ui.label(egui::RichText::new(format!("Showing the first {} of {}.", shown, format_with_commas(total as f64, 0))).color(tk.muted));
                }
            });
            d.footer(|ui| {
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.row_details_open = false;
        }
    }

    /// Keeps every new status message (newest last, at most 50) for the "Messages" list.
    pub(crate) fn log_status_message(&mut self) {
        if self.status_message.is_empty() || self.status_message == self.status_logged {
            return;
        }
        self.status_logged = self.status_message.clone();
        self.status_log.push((chrono::Local::now().format("%H:%M:%S").to_string(), self.status_message.clone()));
        let extra = self.status_log.len().saturating_sub(MESSAGE_LOG_LIMIT);
        self.status_log.drain(..extra);
    }

    /// "Messages": the last 50 status messages, newest first, each in full.
    pub(crate) fn render_messages(&mut self, ctx: &egui::Context) {
        if !self.messages_open {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        theme::modal("Messages").default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                if self.status_log.is_empty() {
                    ui.label(egui::RichText::new("No messages yet.").color(tk.muted));
                }
                for (time, text) in self.status_log.iter().rev() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(theme::mono(time.clone()).color(tk.muted));
                        ui.label(theme::icons_in_text(text));
                    });
                    ui.separator();
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });
        if close {
            self.messages_open = false;
        }
    }
}
