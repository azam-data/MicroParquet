//! Screen module `ftp` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;

/// State of this screen (moved out of `MicroParquetApp` by R1).
pub struct FtpState {
    pub opn_col: String,
    pub mat_col: String,
    pub bal_col: String,
    pub rate_col: String,
    pub is_liability: bool,
    pub curve: Vec<FtpPoint>,
    pub interpolation: FtpInterpolation,
    /// Where the curve comes from (Phase 4 V4).
    pub curve_source: CurveSource,
    pub fixed_date: String,
    pub use_earliest: bool,
    /// The curves of `portable_data/curves`, read when the dialog opens.
    pub store: micro_parquet::curves::CurveStore,
}

/// The three ways an FTP step gets its curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveSource {
    /// The points typed in the dialog.
    Typed,
    /// The curve of the store on or before one date.
    FixedDate,
    /// Each deal's own curve, by its open date.
    Origination,
}

impl FtpState {
    pub fn curve_mode(&self) -> FtpCurveMode {
        match self.curve_source {
            CurveSource::Typed => FtpCurveMode::Inline,
            CurveSource::FixedDate => FtpCurveMode::FixedDate { date: self.fixed_date.trim().to_string() },
            CurveSource::Origination => FtpCurveMode::OriginationDate { use_earliest_for_older: self.use_earliest },
        }
    }

    pub fn set_curve_mode(&mut self, mode: &FtpCurveMode) {
        match mode {
            FtpCurveMode::Inline => self.curve_source = CurveSource::Typed,
            FtpCurveMode::FixedDate { date } => {
                self.curve_source = CurveSource::FixedDate;
                self.fixed_date = date.clone();
            }
            FtpCurveMode::OriginationDate { use_earliest_for_older } => {
                self.curve_source = CurveSource::Origination;
                self.use_earliest = *use_earliest_for_older;
            }
        }
    }

    /// Reads the curve folder again.
    pub fn reload_store(&mut self) {
        self.store = micro_parquet::curves::CurveStore::load(&micro_parquet::curves::curves_dir());
    }
}

impl Default for FtpState {
    fn default() -> Self {
        Self {
            opn_col: "Opndat".to_string(),
            mat_col: "Matdat".to_string(),
            bal_col: "Balance".to_string(),
            rate_col: "Int Rate".to_string(),
            is_liability: true,
            curve: default_bd_ftp_curve(),
            interpolation: FtpInterpolation::Linear,
            curve_source: CurveSource::Typed,
            fixed_date: String::new(),
            use_earliest: false,
            store: micro_parquet::curves::CurveStore::default(),
        }
    }
}

impl MicroParquetApp {
    #[allow(unused_variables)]
    pub(crate) fn render_ftp_modal(&mut self, ctx: &egui::Context, avail_cols: Vec<String>) -> ModalOutcome {
        let mut close_modal = false;
        let mut step_to_apply: Option<(TransformStep, bool)> = None;
        let mut open_curves = false;
        let curve_problem = self.ftp_curve_problem();
        theme::modal("\u{E538} Automated FTP Yield Curve Tenor Mapper")
            .collapsible(false)
            .show(ctx, |d| {
                d.body(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Open Date:");
                        egui::ComboBox::from_id_salt("ftp_opn")
                            .selected_text(&self.ftp.opn_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.ftp.opn_col, c.clone(), c);
                                }
                            });
                        ui.label("Maturity Date:");
                        egui::ComboBox::from_id_salt("ftp_mat")
                            .selected_text(&self.ftp.mat_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.ftp.mat_col, c.clone(), c);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Balance Col:");
                        egui::ComboBox::from_id_salt("ftp_bal")
                            .selected_text(&self.ftp.bal_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.ftp.bal_col, c.clone(), c);
                                }
                            });
                        ui.label("Int Rate Col:");
                        egui::ComboBox::from_id_salt("ftp_rt")
                            .selected_text(&self.ftp.rate_col)
                            .show_ui(ui, |ui| {
                                for c in &avail_cols {
                                    ui.selectable_value(&mut self.ftp.rate_col, c.clone(), c);
                                }
                            });
                    });
                    ui.checkbox(
                        &mut self.ftp.is_liability,
                        "Liability Book (Deposits/Borrowings: Spread = FTP Rate - Account Rate)",
                    );
                    ui.separator();
                    ui.strong("Yield curve:");
                    ui.radio_value(&mut self.ftp.curve_source, CurveSource::Typed, "The curve typed below");
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(&mut self.ftp.curve_source, CurveSource::FixedDate, "The curve on or before one date:");
                        ui.add_enabled(self.ftp.curve_source == CurveSource::FixedDate, egui::TextEdit::singleline(&mut self.ftp.fixed_date).hint_text("YYYY-MM-DD").desired_width(theme::GAP_COMBO_NARROW_W));
                    });
                    ui.radio_value(&mut self.ftp.curve_source, CurveSource::Origination, "Each deal's own curve: the latest curve on or before its open date");
                    ui.add_enabled(self.ftp.curve_source == CurveSource::Origination, egui::Checkbox::new(&mut self.ftp.use_earliest, "Use the earliest curve for older deals"));
                    let tk = *theme::tokens(self.dark_mode);
                    let info = if self.ftp.curve_source == CurveSource::Typed {
                        None
                    } else {
                        let s = &self.ftp.store;
                        Some(match (s.curves.first(), s.curves.last()) {
                            (Some(a), Some(b)) => format!("{} curve(s) in portable_data/curves, dated {} to {}.", s.curves.len(), a.date, b.date),
                            _ => "There are no curves in portable_data/curves yet.".to_string(),
                        })
                    };
                    ui.horizontal_wrapped(|ui| {
                        if let Some(i) = &info {
                            ui.label(egui::RichText::new(i).color(tk.muted));
                        }
                        if ui.button(format!("{}  Yield curves\u{2026}", theme::ph::CHART_LINE)).clicked() {
                            open_curves = true;
                        }
                    });
                    if let Some(p) = self.ftp_curve_problem() {
                        theme::error_label(ui, &p);
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Rate between tenor points:");
                        for m in [FtpInterpolation::Linear, FtpInterpolation::NextPointUp] {
                            ui.radio_value(&mut self.ftp.interpolation, m, m.label());
                        }
                    });
                    if self.ftp.curve_source == CurveSource::Typed {
                        ui.label("Below the first point the first rate applies; beyond the last point the last rate. The FTP_Method column shows which rule each row used.");
                        ui.strong("Benchmark Yield Curve (Tenor Days -> FTP Rate %):");
                        for pt in &mut self.ftp.curve {
                            ui.horizontal(|ui| {
                                ui.label("Tenor Days:");
                                ui.add(egui::DragValue::new(&mut pt.tenor_days));
                                ui.label("FTP %:");
                                ui.add(egui::DragValue::new(&mut pt.ftp_rate).speed(0.05)); // not a size: drag speed
                            });
                        }
                    } else {
                        ui.label("Rates between a curve's points are interpolated as chosen above; the FTP_Curve_Date column says which curve priced each deal.");
                    }
                });
                d.footer(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        let go = ui.add_enabled_ui(curve_problem.is_none(), |ui| theme::primary_button(ui, "\u{E182} Map FTP Rates & Compute Margin")).inner;
                        if go.clicked() {
                            step_to_apply = self.tool_step(&ActiveToolModal::FtpCurveMap).map(|s| (s, false));
                            close_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        if open_curves {
            self.open_yield_curves();
        }
        ModalOutcome { close: close_modal, step: step_to_apply }
    }

    /// What is wrong with the curve choice, or `None`: a fixed date that is not a date or that no
    /// curve is dated before, or an empty store.
    pub(crate) fn ftp_curve_problem(&self) -> Option<String> {
        let f = &self.ftp;
        match f.curve_source {
            CurveSource::Typed => None,
            CurveSource::Origination => {
                if let Some((file, reason)) = f.store.problems.first() {
                    return Some(format!("The curve file {} cannot be used: {}", file, reason));
                }
                if f.store.curves.is_empty() {
                    Some("There are no curves yet. Add one in Yield curves, or type the curve.".to_string())
                } else {
                    None
                }
            }
            CurveSource::FixedDate => {
                if let Some((file, reason)) = f.store.problems.first() {
                    return Some(format!("The curve file {} cannot be used: {}", file, reason));
                }
                let Ok(d) = chrono::NaiveDate::parse_from_str(f.fixed_date.trim(), "%Y-%m-%d") else {
                    return Some("Type the curve date as YYYY-MM-DD (for example 2026-06-30).".to_string());
                };
                match f.store.on_or_before(d) {
                    Some(_) => None,
                    None => Some(format!("No curve is dated on or before {}. Add one in Yield curves.", d)),
                }
            }
        }
    }
}
