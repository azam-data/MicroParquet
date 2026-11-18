//! Banking ▸ Loans & credit ▸ "IFRS 9 ECL" (Phase 5 R7): the columns of the loan tab, the cutoff
//! date and the staging options make the step that adds `Stage`, `Stage_Reason`, `EAD`, `ECL`,
//! `ECL_Rule_File` and `ECL_Params_File`; a second button also opens the §9.1 summary tab.

use crate::ui::pick::{col_combo, field_label, guess_column, text_combo, DialogRects};
use crate::ui::prelude::*;
use micro_parquet::ecl::{load_params, load_policy, load_staging, EclSummarySpec};

pub const IFRS9_TITLE: &str = "IFRS 9 ECL (BRPD 06/2026)";

/// The files read for the typed cutoff date: what the dialog says about them.
#[derive(Clone, Debug, PartialEq)]
pub struct EclFiles {
    pub staging_file: String,
    pub params_file: String,
    pub sample: bool,
    pub source: String,
    pub segments: Vec<String>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
    pub classified_default: bool,
}

pub struct EclState {
    pub segment_by_col: bool,
    pub segment_col: String,
    pub fixed_segment: String,
    pub drawn_col: String,
    pub undrawn_col: String,
    pub eir_col: String,
    pub maturity_col: String,
    pub dpd_col: String,
    pub grade_col: String,
    pub classified_stage3: bool,
    pub forborne_col: String,
    pub unassessed_col: String,
    pub rating_orig_col: String,
    pub rating_now_col: String,
    pub prev_col: String,
    pub days_col: String,
    /// The regulatory provision column of the Provision step, for the summary.
    pub provision_col: String,
    pub cutoff: String,
    pub files: Option<(String, Result<EclFiles, String>)>,
    pub rects: DialogRects,
}

impl Default for EclState {
    fn default() -> Self {
        Self {
            segment_by_col: false,
            segment_col: String::new(),
            fixed_segment: String::new(),
            drawn_col: String::new(),
            undrawn_col: String::new(),
            eir_col: String::new(),
            maturity_col: String::new(),
            dpd_col: String::new(),
            grade_col: String::new(),
            classified_stage3: true,
            forborne_col: String::new(),
            unassessed_col: String::new(),
            rating_orig_col: String::new(),
            rating_now_col: String::new(),
            prev_col: String::new(),
            days_col: String::new(),
            provision_col: String::new(),
            cutoff: String::new(),
            files: None,
            rects: DialogRects::default(),
        }
    }
}

impl MicroParquetApp {
    pub(crate) fn open_ifrs9(&mut self) {
        let cols = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let s = &mut self.ecl;
        if s.cutoff.trim().is_empty() {
            s.cutoff = crate::ui::cashflow_ladder::last_month_end(chrono::Local::now().date_naive());
        }
        s.segment_col = guess_column(&cols, &["segment", "sector", "portfolio"]);
        s.segment_by_col = !s.segment_col.is_empty();
        s.drawn_col = guess_column(&cols, &["drawn", "balance", "outstanding"]);
        s.undrawn_col = guess_column(&cols, &["undrawn"]);
        s.eir_col = guess_column(&cols, &["eir", "effective", "int_rate", "rate"]);
        s.maturity_col = guess_column(&cols, &["matdat", "maturity"]);
        s.dpd_col = guess_column(&cols, &["dpd", "days_past", "overdue_days"]);
        s.grade_col = if cols.iter().any(|c| c == "CL_Final") { "CL_Final".to_string() } else { guess_column(&cols, &["cl_grade", "cl_status", "classification"]) };
        s.forborne_col = guess_column(&cols, &["forborne", "forbear", "restructur"]);
        s.unassessed_col = guess_column(&cols, &["sicr"]);
        s.rating_orig_col = guess_column(&cols, &["rating_orig", "orig_rating", "rating_at_orig"]);
        s.rating_now_col = guess_column(&cols, &["rating_now", "current_rating", "rating_current"]);
        s.prev_col = guess_column(&cols, &["prev_stage", "previous_stage"]);
        s.days_col = guess_column(&cols, &["days_performing", "performing"]);
        s.provision_col = guess_column(&cols, &["required_provision", "provision_required"]);
        if let Ok(p) = load_policy() {
            s.classified_stage3 = p.data.classified_stage3;
        }
        s.files = None;
        self.active_tool = ActiveToolModal::IfrsEcl;
    }

    /// Reads the staging and parameter files for the typed cutoff date again when the date changed.
    fn refresh_ecl_files(&mut self) {
        let key = self.ecl.cutoff.trim().to_string();
        if matches!(&self.ecl.files, Some((k, _)) if *k == key) {
            return;
        }
        let r = match chrono::NaiveDate::parse_from_str(&key, "%Y-%m-%d") {
            Err(_) => Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string()),
            Ok(d) => load_staging(d).map_err(|e| format!("{:#}", e)).and_then(|st| {
                load_params(d).map_err(|e| format!("{:#}", e)).and_then(|(p, warnings)| {
                    let policy = load_policy().map_err(|e| format!("{:#}", e))?;
                    Ok(EclFiles {
                        staging_file: st.meta.file.clone(),
                        params_file: p.meta.file.clone(),
                        sample: p.meta.sample || policy.sample,
                        source: p.meta.source.clone(),
                        segments: p.data.segments.iter().map(|s| s.name.clone()).collect(),
                        notes: policy.data.notes.clone(),
                        warnings,
                        classified_default: policy.data.classified_stage3,
                    })
                })
            }),
        };
        if let Ok(f) = &r {
            if self.ecl.fixed_segment.trim().is_empty() || !f.segments.contains(&self.ecl.fixed_segment) {
                self.ecl.fixed_segment = f.segments.first().cloned().unwrap_or_default();
            }
        }
        self.ecl.files = Some((key, r));
    }

    /// The step the dialog would make, or the first thing that is missing.
    pub(crate) fn ifrs9_step(&self) -> Result<TransformStep, String> {
        let s = &self.ecl;
        let cutoff = s.cutoff.trim().to_string();
        if chrono::NaiveDate::parse_from_str(&cutoff, "%Y-%m-%d").is_err() {
            return Err("Type the cutoff date as YYYY-MM-DD (for example 2026-10-31).".to_string());
        }
        if let Some((k, Err(e))) = &s.files {
            if *k == cutoff {
                return Err(e.clone());
            }
        }
        let some = |v: &str| if v.trim().is_empty() { None } else { Some(v.trim().to_string()) };
        if s.segment_by_col && s.segment_col.trim().is_empty() {
            return Err("Choose the segment column, or choose the same segment for every row.".to_string());
        }
        if !s.segment_by_col && s.fixed_segment.trim().is_empty() {
            return Err("Choose the segment of the rows.".to_string());
        }
        for (v, what) in [(&s.drawn_col, "drawn amount"), (&s.eir_col, "effective interest rate"), (&s.maturity_col, "maturity date")] {
            if v.trim().is_empty() {
                return Err(format!("Choose the {} column.", what));
            }
        }
        if s.rating_orig_col.trim().is_empty() != s.rating_now_col.trim().is_empty() {
            return Err("Choose both rating columns (at origination and current), or neither.".to_string());
        }
        if s.prev_col.trim().is_empty() != s.days_col.trim().is_empty() {
            return Err("Choose both the previous stage and the days performing, or neither.".to_string());
        }
        Ok(TransformStep::IfrsEcl {
            segment_col: if s.segment_by_col { some(&s.segment_col) } else { None },
            fixed_segment: if s.segment_by_col { String::new() } else { s.fixed_segment.trim().to_string() },
            drawn_col: s.drawn_col.trim().to_string(),
            undrawn_col: some(&s.undrawn_col),
            eir_col: s.eir_col.trim().to_string(),
            maturity_col: s.maturity_col.trim().to_string(),
            dpd_col: some(&s.dpd_col),
            grade_col: some(&s.grade_col),
            classified_stage3: s.classified_stage3,
            forborne_col: some(&s.forborne_col),
            sicr_unassessed_col: some(&s.unassessed_col),
            rating_orig_col: some(&s.rating_orig_col),
            rating_now_col: some(&s.rating_now_col),
            prev_stage_col: some(&s.prev_col),
            days_performing_col: some(&s.days_col),
            cutoff_date: cutoff,
        })
    }

    pub(crate) fn render_ifrs9_modal(&mut self, ctx: &egui::Context, cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let mut step: Option<(TransformStep, bool)> = None;
        let mut with_summary = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  {}", theme::ph::BANK, IFRS9_TITLE);
        let mut rects = DialogRects::default();
        self.refresh_ecl_files();
        let problem = self.ifrs9_step().err();
        let files: Option<EclFiles> = match &self.ecl.files {
            Some((_, Ok(f))) => Some(f.clone()),
            _ => None,
        };
        let segments = files.as_ref().map(|f| f.segments.clone()).unwrap_or_default();
        let summary_problem = if self.ecl.provision_col.trim().is_empty() { Some("Choose the regulatory provision column to open the summary.") } else { None };
        theme::modal(&title).default_width(theme::DIALOG_W_PIVOT).fill_height().show(ctx, |d| {
            d.header(|ui| {
                if files.as_ref().is_some_and(|f| f.sample) {
                    let r = ui.scope(|ui| theme::warning_label(ui, "Sample parameters: the ECL parameters in use (PD curves, LGD, CCF, scenarios) or the bank policy (policy/ifrs9_bank_policy.json) are a sample. Replace them with the bank's own before the ECL goes into a return."));
                    rects.banner = r.response.rect;
                }
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Cutoff date (YYYY-MM-DD)");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.ecl.cutoff).desired_width(theme::CF_DEFAULT_W));
                    rects.controls.push(("Cutoff date".to_string(), r.rect));
                });
                if let Some(p) = &problem {
                    theme::error_label(ui, p);
                }
            });
            d.body_scroll(|ui| {
                let s = &mut self.ecl;
                ui.horizontal_wrapped(|ui| {
                    field_label(ui, &tk, "Segment");
                    let a = ui.radio_value(&mut s.segment_by_col, false, "Same for every row:");
                    rects.controls.push(("Same segment".to_string(), a.rect));
                    ui.add_enabled_ui(!s.segment_by_col, |ui| {
                        let r = text_combo(ui, "ecl_seg_fixed", &mut s.fixed_segment, &segments, theme::CF_COMBO_W);
                        rects.controls.push(("Fixed segment".to_string(), r));
                    });
                    let b = ui.radio_value(&mut s.segment_by_col, true, "From the column:");
                    rects.controls.push(("Segment from column".to_string(), b.rect));
                    ui.add_enabled_ui(s.segment_by_col, |ui| {
                        let r = col_combo(ui, "ecl_seg_col", &mut s.segment_col, &cols, false);
                        rects.controls.push(("Segment column".to_string(), r));
                    });
                });
                let c = ui.checkbox(&mut s.classified_stage3, "Classified grades (SS, DF, B/L) are Stage 3");
                rects.controls.push(("Classified are Stage 3".to_string(), c.rect));
                ui.add_space(theme::SPACE_S);
                egui::Grid::new("ecl_roles").num_columns(4).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                    let mut row = |ui: &mut egui::Ui, id: &str, label: &str, value: &mut String, optional: bool, rects: &mut DialogRects| {
                        ui.label(label);
                        let r = col_combo(ui, id, value, &cols, optional);
                        rects.controls.push((label.to_string(), r));
                    };
                    row(ui, "ecl_drawn", "Drawn amount", &mut s.drawn_col, false, &mut rects);
                    row(ui, "ecl_undrawn", "Undrawn amount", &mut s.undrawn_col, true, &mut rects);
                    ui.end_row();
                    row(ui, "ecl_eir", "Effective interest rate (%)", &mut s.eir_col, false, &mut rects);
                    row(ui, "ecl_mat", "Maturity date", &mut s.maturity_col, false, &mut rects);
                    ui.end_row();
                    row(ui, "ecl_dpd", "Days past due", &mut s.dpd_col, true, &mut rects);
                    row(ui, "ecl_grade", "Classification grade", &mut s.grade_col, true, &mut rects);
                    ui.end_row();
                    row(ui, "ecl_forb", "Forborne (Y/N)", &mut s.forborne_col, true, &mut rects);
                    row(ui, "ecl_unass", "SICR not assessed (Y/N)", &mut s.unassessed_col, true, &mut rects);
                    ui.end_row();
                    row(ui, "ecl_r0", "Rating at origination", &mut s.rating_orig_col, true, &mut rects);
                    row(ui, "ecl_r1", "Current rating", &mut s.rating_now_col, true, &mut rects);
                    ui.end_row();
                    row(ui, "ecl_prev", "Previous stage", &mut s.prev_col, true, &mut rects);
                    row(ui, "ecl_days", "Days performing", &mut s.days_col, true, &mut rects);
                    ui.end_row();
                    row(ui, "ecl_prov", "Regulatory provision (summary)", &mut s.provision_col, true, &mut rects);
                    ui.end_row();
                });
                ui.add_space(theme::SPACE_S);
                let notes = ui.scope(|ui| {
                    ui.set_max_width(ui.available_width());
                    let muted = |t: &str| egui::Label::new(egui::RichText::new(t).color(tk.muted).size(theme::TEXT_LABEL)).wrap();
                    if let Some(f) = &files {
                        ui.add(muted(&format!("Staging rules: {}. Parameters: {} ({}).", f.staging_file, f.params_file, f.source)));
                        for n in f.notes.iter().chain(f.warnings.iter()) {
                            ui.add(muted(n));
                        }
                    }
                    ui.add(muted("Stage 3: 90+ days past due, or a classified grade when the box is ticked; Stage 2: 30+ days past due, forborne, SICR not assessed, or a rating downgrade; otherwise Stage 1. A better stage than the previous one applies one stage at a time, after 90 days performing (365 for forborne exposures). Stage 1 ECL is the first year, Stage 2 the lifetime, Stage 3 LGD \u{d7} EAD; each year is discounted at the effective interest rate."));
                });
                rects.notes = notes.response.rect;
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = ui.add_enabled_ui(problem.is_none(), |ui| theme::primary_button(ui, "Calculate IFRS 9 ECL")).inner;
                    rects.buttons.push(("Calculate IFRS 9 ECL".to_string(), b.rect));
                    if b.clicked() {
                        step = self.ifrs9_step().ok().map(|s| (s, false));
                        close = true;
                    }
                    let b2 = ui.add_enabled_ui(problem.is_none() && summary_problem.is_none(), |ui| ui.button("Calculate ECL and open the summary")).inner;
                    let b2 = match summary_problem {
                        Some(p) => b2.on_disabled_hover_text(p),
                        None => b2,
                    };
                    rects.buttons.push(("Calculate ECL and open the summary".to_string(), b2.rect));
                    if b2.clicked() {
                        step = self.ifrs9_step().ok().map(|s| (s, false));
                        with_summary = true;
                        close = true;
                    }
                    let c = ui.button("Cancel");
                    rects.buttons.push(("Cancel".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.ecl.rects = rects;
        if with_summary {
            if let (Some((st, _)), Some(tab)) = (&step, self.tabs.get(self.active_tab)) {
                let mut steps = tab.steps.clone();
                steps.push(st.clone());
                let source = TabSource::EclSummary { source: Box::new(tab.source.clone()), steps, spec: EclSummarySpec { grade_col: self.ecl.grade_col.trim().to_string(), provision_col: self.ecl.provision_col.trim().to_string() } };
                self.pending_ecl_summary = Some(source);
            }
        }
        ModalOutcome { close, step }
    }
}
