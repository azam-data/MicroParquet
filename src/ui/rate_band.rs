//! Screen module `rate_band`: the Rate profile tool (Banking), which puts a rate column into
//! labelled bands ("09. 8%–9%"), like the maturity bucketer does for days to maturity.

use crate::ui::prelude::*;

/// State of this screen.
pub struct RateBandState {
    pub rate_col: String,
    /// Custom edges (a list) instead of fixed-width bands.
    pub custom: bool,
    pub start: f64,
    pub end: f64,
    pub step: f64,
    /// The custom edges as typed ("0, 5, 7, 9, 12").
    pub edges_text: String,
    /// Bands are [a, b) instead of (a, b].
    pub lower_inclusive: bool,
    pub new_col: String,
    pub presets: Vec<RateBandDef>,
    /// The name a saved preset gets (empty: the profile is not named).
    pub preset_name: String,
    /// "Number the bands (09. 8%–9%)": off for new steps (W3); an edited step keeps its own.
    pub numbered: bool,
    /// Where the tick box was drawn (tests).
    pub numbered_rect: Option<egui::Rect>,
}

impl Default for RateBandState {
    fn default() -> Self {
        let d = RateBandDef::default_profile();
        let mut s = Self {
            rate_col: "Rate".to_string(),
            custom: false,
            start: 0.0,
            end: 15.0, // not a size: percent
            step: 1.0,
            edges_text: "0, 5, 7, 9, 12".to_string(),
            lower_inclusive: false,
            new_col: "Rate_Band".to_string(),
            presets: RateBandDef::default_presets(),
            preset_name: String::new(),
            numbered: false,
            numbered_rect: None,
        };
        s.load(&d);
        s.preset_name.clear();
        s
    }
}

impl RateBandState {
    /// The profile the settings describe, or why they do not make one.
    pub fn def(&self) -> anyhow::Result<RateBandDef> {
        let name = if self.preset_name.trim().is_empty() { self.describe() } else { self.preset_name.trim().to_string() };
        let mode = if self.custom {
            RateBandMode::Custom { edges: parse_rate_edges(&self.edges_text)? }
        } else {
            RateBandMode::Fixed { start_pct: self.start, end_pct: self.end, step_pct: self.step }
        };
        let def = RateBandDef { name, mode, lower_inclusive: self.lower_inclusive, numbered: self.numbered };
        def.edges()?;
        Ok(def)
    }

    /// A short name for the settings: "0% to 15% by 1%" or "Edges 0, 5, 7, 9, 12".
    pub fn describe(&self) -> String {
        if self.custom {
            let e = parse_rate_edges(&self.edges_text).unwrap_or_default();
            format!("Edges {}", e.iter().map(|x| fmt_pct(*x)).collect::<Vec<_>>().join(", "))
        } else {
            format!("{}% to {}% by {}%", fmt_pct(self.start), fmt_pct(self.end), fmt_pct(self.step))
        }
    }

    /// Puts a profile's settings into the dialog.
    pub fn load(&mut self, def: &RateBandDef) {
        match &def.mode {
            RateBandMode::Fixed { start_pct, end_pct, step_pct } => {
                self.custom = false;
                (self.start, self.end, self.step) = (*start_pct, *end_pct, *step_pct);
            }
            RateBandMode::Custom { edges } => {
                self.custom = true;
                self.edges_text = edges.iter().map(|x| fmt_pct(*x)).collect::<Vec<_>>().join(", ");
            }
        }
        self.lower_inclusive = def.lower_inclusive;
        self.preset_name = def.name.clone();
    }
}

impl MicroParquetApp {
    pub(crate) fn render_rate_band_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        let tk = *theme::tokens(self.dark_mode);
        // a rate column that is not in this tab: the first one named like a rate
        if !avail_cols.contains(&self.rate_band.rate_col) {
            if let Some(c) = avail_cols.iter().find(|c| c.to_lowercase().contains("rate")).or(avail_cols.first()) {
                self.rate_band.rate_col = c.clone();
            }
        }
        let current = self.rate_band.def();
        theme::modal("\u{E0B4} Rate profile: put a rate column into bands")
            .collapsible(false)
            .default_width(theme::DIALOG_W_TOOL)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.set_max_width(theme::DIALOG_W_TOOL - 2.0 * theme::SPACE_M - theme::DIALOG_SCROLL_BAR_ALLOWANCE);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Rate column (percent):");
                        theme::sized_combo(ui, theme::COND_COMBO_W, |ui| {
                            egui::ComboBox::from_id_salt("rb_col")
                                .width(theme::COND_COMBO_W)
                                .selected_text(&self.rate_band.rate_col)
                                .show_ui(ui, |ui| {
                                    for c in &avail_cols {
                                        ui.selectable_value(&mut self.rate_band.rate_col, c.clone(), c);
                                    }
                                });
                        });
                        ui.label("Output column:");
                        ui.add(egui::TextEdit::singleline(&mut self.rate_band.new_col).desired_width(theme::FIELD_W_COLUMN));
                    });
                    ui.add_space(theme::SPACE_S);

                    // presets: saved next to the bucket presets in portable_data
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Preset:");
                        let mut picked: Option<RateBandDef> = None;
                        theme::sized_combo(ui, theme::RATE_PRESET_W, |ui| {
                            egui::ComboBox::from_id_salt("rb_preset")
                                .width(theme::RATE_PRESET_W)
                                .selected_text(if self.rate_band.preset_name.is_empty() { "choose a preset" } else { self.rate_band.preset_name.as_str() })
                                .show_ui(ui, |ui| {
                                    for p in &self.rate_band.presets {
                                        if ui.selectable_label(p.name == self.rate_band.preset_name, &p.name).clicked() {
                                            picked = Some(p.clone());
                                        }
                                    }
                                });
                        });
                        if let Some(p) = picked {
                            self.rate_band.load(&p);
                        }
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut self.rate_band.preset_name).desired_width(theme::FIELD_W_SORT_COLUMN));
                        let can_save = current.is_ok() && !self.rate_band.preset_name.trim().is_empty();
                        if ui.add_enabled(can_save, egui::Button::new("\u{E248} Save preset")).on_hover_text("Saved in portable_data/rate_bands.json").clicked() {
                            if let Ok(def) = self.rate_band.def() {
                                let file = self.paths.rate_bands_file();
                                match RateBandDef::save_custom_preset(&file, def.clone()) {
                                    Ok(()) => {
                                        self.rate_band.presets = RateBandDef::load_all_presets(&file);
                                        self.status_message = format!("Saved the rate profile preset '{}'.", def.name);
                                    }
                                    Err(e) => self.status_message = format!("Could not save the preset: {:#}", e),
                                }
                            }
                        }
                    });
                    ui.separator();

                    // the bands
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.rate_band.custom, false, "Fixed width");
                        ui.selectable_value(&mut self.rate_band.custom, true, "Custom edges");
                    });
                    if self.rate_band.custom {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Edges (%):");
                            ui.add(egui::TextEdit::singleline(&mut self.rate_band.edges_text).desired_width(theme::FIELD_W_SORT_COLUMN).hint_text("0, 5, 7, 9, 12"));
                        });
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Start %");
                            ui.add(egui::DragValue::new(&mut self.rate_band.start).speed(theme::RATE_DRAG_SPEED).range(theme::RATE_PCT_RANGE));
                            ui.label("End %");
                            ui.add(egui::DragValue::new(&mut self.rate_band.end).speed(theme::RATE_DRAG_SPEED).range(theme::RATE_PCT_RANGE));
                            ui.label("Step %");
                            ui.add(egui::DragValue::new(&mut self.rate_band.step).speed(theme::RATE_STEP_DRAG_SPEED).range(theme::RATE_STEP_RANGE));
                        });
                    }
                    ui.add_space(theme::SPACE_S);
                    ui.label("A rate on a band's edge goes to:");
                    ui.radio_value(&mut self.rate_band.lower_inclusive, false, "the lower band: (a, b], so 9.00 falls in 8%\u{2013}9%");
                    ui.radio_value(&mut self.rate_band.lower_inclusive, true, "the upper band: [a, b), so 9.00 falls in 9%\u{2013}10%");
                    ui.add_space(theme::SPACE_S);
                    let r = ui
                        .checkbox(&mut self.rate_band.numbered, "Number the bands (09. 8%\u{2013}9%)")
                        .on_hover_text("Off: the labels read 8%\u{2013}9% and pivots, charts and sorts put them in band order. On: the labels start with a number, as before Phase 8, so they also sort in band order as plain text.");
                    self.rate_band.numbered_rect = Some(r.rect);
                    ui.separator();

                    match &current {
                        Ok(def) => match def.labels() {
                            Ok(l) => {
                                let mut all: Vec<&str> = vec![l.zero.as_str()];
                                all.extend(l.bands.iter().map(|s| s.as_str()));
                                all.extend([l.above.as_str(), l.below.as_str(), l.blank.as_str()]);
                                let shown = all.len().min(theme::RATE_LABELS_SHOWN);
                                let mut text = all[..shown].join("  \u{b7}  ");
                                if all.len() > shown {
                                    text.push_str(&format!("  \u{b7}  \u{2026} {} more", all.len() - shown));
                                }
                                ui.label(egui::RichText::new(format!("{} bands:", l.bands.len())).color(tk.muted));
                                ui.label(egui::RichText::new(text).color(tk.ink2));
                            }
                            Err(e) => theme::error_label(ui, &format!("{:#}", e)),
                        },
                        Err(e) => theme::error_label(ui, &format!("{:#}", e)),
                    }
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        let ok = current.is_ok() && !self.rate_band.new_col.trim().is_empty();
                        let generate = ui.add_enabled_ui(ok, |ui| theme::primary_button(ui, "\u{E182} Add Rate Profile")).inner;
                        if generate.clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::RateBand).map(|s| (s, false));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        ModalOutcome { close: close_modal, step: step_to_apply }
    }
}
