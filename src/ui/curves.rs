//! The yield-curve manager (Phase 4 V4): lists the curves of `portable_data/curves`, edits the
//! points of one, imports a two-column CSV and saves, duplicates to a new date or deletes a curve.

use crate::ui::prelude::*;
use chrono::NaiveDate;
use micro_parquet::curves::{
    curves_dir, delete_curve, parse_curve_csv, save_curve, validate_points, CurveStore, YieldCurve,
};

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug)]
pub struct CurveRects {
    pub list: egui::Rect,
    pub list_rows: Vec<(String, egui::Rect)>,
    pub editor: egui::Rect,
    pub point_rows: Vec<egui::Rect>,
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub message: egui::Rect,
    /// The warning above the fields of a stored curve.
    pub warning: egui::Rect,
}

impl Default for CurveRects {
    fn default() -> Self {
        let n = egui::Rect::NOTHING;
        Self { list: n, list_rows: Vec::new(), editor: n, point_rows: Vec::new(), fields: Vec::new(), buttons: Vec::new(), message: n, warning: n }
    }
}

/// The curve being edited, as typed.
#[derive(Clone, Debug, Default)]
pub struct CurveEditor {
    pub date: String,
    pub currency: String,
    pub source: String,
    pub points: Vec<FtpPoint>,
    /// The date of the file this was opened from (`None` for a new curve).
    pub opened_from: Option<NaiveDate>,
}

#[derive(Default)]
pub struct CurvesState {
    pub store: CurveStore,
    pub editor: CurveEditor,
    /// A line under the editor: (is it an error?, text).
    pub message: Option<(bool, String)>,
    pub confirm_delete: bool,
    /// "Save curve" would change a curve that is already stored: asks first.
    pub confirm_overwrite: bool,
    pub rects: CurveRects,
}

impl CurvesState {
    /// Reads the folder again.
    pub fn reload(&mut self) {
        self.store = CurveStore::load(&curves_dir());
    }

    /// The curve as typed, or what is wrong with it.
    pub fn typed_curve(&self) -> Result<YieldCurve, String> {
        let e = &self.editor;
        let date = NaiveDate::parse_from_str(e.date.trim(), "%Y-%m-%d").map_err(|_| "Type the curve's date as YYYY-MM-DD (for example 2026-01-01).".to_string())?;
        if e.currency.trim().is_empty() {
            return Err("Type the curve's currency (for example BDT). Every curve needs one.".to_string());
        }
        validate_points(&e.points).map_err(|err| format!("{:#}", err))?;
        Ok(YieldCurve { date, currency: e.currency.trim().to_string(), source: e.source.trim().to_string(), points: e.points.clone() })
    }

    /// Opens a curve of the store in the editor.
    pub fn open(&mut self, date: NaiveDate) {
        if let Some(c) = self.store.curves.iter().find(|c| c.date == date) {
            self.editor = CurveEditor { date: c.date.format("%Y-%m-%d").to_string(), currency: c.currency.clone(), source: c.source.clone(), points: c.points.clone(), opened_from: Some(c.date) };
            self.message = None;
            self.confirm_delete = false;
            self.confirm_overwrite = false;
        }
    }

    /// A new curve: today's currency, no date, two sample rows to overwrite.
    pub fn new_curve(&mut self) {
        self.editor = CurveEditor { currency: "BDT".to_string(), points: vec![FtpPoint { tenor_days: 30, ftp_rate: 0.0 }, FtpPoint { tenor_days: 365, ftp_rate: 0.0 }], ..Default::default() };
        self.message = None;
        self.confirm_delete = false;
        self.confirm_overwrite = false;
    }

    /// Replaces the points with those of a CSV; the reason when the text is not a curve.
    pub fn import_csv_text(&mut self, text: &str) -> Result<usize, String> {
        match parse_curve_csv(text) {
            Ok(points) => {
                let n = points.len();
                self.editor.points = points;
                self.message = Some((false, format!("Imported {} points. Check them, set the date and save.", n)));
                Ok(n)
            }
            Err(e) => {
                let msg = format!("{:#}", e);
                self.message = Some((true, format!("The CSV was not used: {}", msg)));
                Err(msg)
            }
        }
    }

    /// Saves the curve in the editor as `<date>.json`. A curve of that date that is not the one
    /// being edited is not overwritten.
    pub fn save(&mut self) -> Result<std::path::PathBuf, String> {
        let curve = self.typed_curve()?;
        if self.editor.opened_from != Some(curve.date) && self.store.curves.iter().any(|c| c.date == curve.date) {
            return Err(format!("A curve dated {} already exists. Select it in the list to change it, or choose another date.", curve.date));
        }
        let path = save_curve(&curves_dir(), &curve).map_err(|e| format!("{:#}", e))?;
        self.reload();
        self.editor.opened_from = Some(curve.date);
        self.message = Some((false, format!("Saved {}.", path.display())));
        Ok(path)
    }

    /// The stored curve the editor was opened from.
    pub fn opened_curve(&self) -> Option<&YieldCurve> {
        let d = self.editor.opened_from?;
        self.store.curves.iter().find(|c| c.date == d)
    }

    /// True when "Save curve" would replace a stored curve with different content.
    pub fn would_change_stored_curve(&self) -> bool {
        match (self.typed_curve(), self.opened_curve()) {
            (Ok(typed), Some(old)) => typed.date == old.date && typed != *old,
            _ => false,
        }
    }

    /// "Save curve" in the dialog: a new date is saved at once; a change to a stored curve first
    /// asks (`confirm_overwrite`), and no change writes nothing.
    pub fn save_asking(&mut self) -> Result<(), String> {
        let typed = self.typed_curve()?;
        if let Some(old) = self.opened_curve() {
            if old.date == typed.date {
                if *old == typed {
                    self.message = Some((false, "Nothing changed, so nothing was saved.".to_string()));
                    return Ok(());
                }
                self.message = None;
                self.confirm_overwrite = true;
                return Ok(());
            }
        }
        self.save().map(|_| ())
    }

    /// Writes the editor's curve under the date typed, leaving the curve it was opened from as it was.
    /// The date must be a new one.
    pub fn save_as_new_date(&mut self) -> Result<std::path::PathBuf, String> {
        let typed = self.typed_curve()?;
        if self.editor.opened_from == Some(typed.date) {
            return Err(format!("The date is still {}. Type the new curve's date first (for example the day the rates changed), then click Save as new date.", typed.date));
        }
        self.save()
    }

    /// Deletes the curve that was opened.
    pub fn delete_opened(&mut self) -> Result<(), String> {
        let Some(date) = self.editor.opened_from else {
            return Err("Select a saved curve to delete.".to_string());
        };
        delete_curve(&curves_dir(), date).map_err(|e| format!("{:#}", e))?;
        self.reload();
        self.new_curve();
        self.message = Some((false, format!("Deleted the curve of {}.", date)));
        Ok(())
    }
}

impl MicroParquetApp {
    pub(crate) fn open_yield_curves(&mut self) {
        self.curves.reload();
        match self.curves.store.curves.last().map(|c| c.date) {
            Some(d) => self.curves.open(d),
            None => self.curves.new_curve(),
        }
        self.active_tool = ActiveToolModal::YieldCurves;
    }

    pub(crate) fn render_curves_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let tk = *theme::tokens(self.dark_mode);
        let title = format!("{}  Yield curves", theme::ph::CHART_LINE);
        let mut rects = CurveRects::default();
        let check = self.curves.typed_curve();
        let mut action: Option<&'static str> = None;
        theme::modal(&title).default_width(theme::CURVES_DIALOG_W).show(ctx, |d| {
            d.body(|ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::PIVOT_GAP;
                    // the list
                    let list = ui.allocate_ui_with_layout(egui::vec2(theme::CURVES_LIST_W, theme::CURVES_BODY_MIN_H), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(egui::RichText::new("CURVES").size(theme::TEXT_LABEL).color(tk.muted).strong());
                        let curves = self.curves.store.curves.clone();
                        let selected = self.curves.editor.opened_from;
                        if curves.is_empty() {
                            ui.label(egui::RichText::new("No curve yet. Make one, or import a CSV.").color(tk.muted));
                        }
                        for c in curves.iter().rev() {
                            let label = format!("{}  \u{b7}  {} {}", c.date.format("%Y-%m-%d"), c.currency, format!("{} points", c.points.len()));
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::PIVOT_FIELD_ROW_H), egui::Sense::click());
                            if selected == Some(c.date) || resp.hovered() {
                                ui.painter().rect_filled(rect, theme::RADIUS_SMALL, if selected == Some(c.date) { tk.accent_soft } else { tk.zebra });
                            }
                            ui.put(rect.shrink2(egui::vec2(theme::SPACE_S, 0.0)), egui::Label::new(egui::RichText::new(&label).color(tk.ink)).truncate().selectable(false));
                            rects.list_rows.push((c.date.format("%Y-%m-%d").to_string(), rect));
                            if resp.clicked() {
                                self.curves.open(c.date);
                            }
                        }
                        if !self.curves.store.problems.is_empty() {
                            ui.add_space(theme::SPACE_S);
                            ui.label(egui::RichText::new("Files that cannot be used").size(theme::TEXT_LABEL).color(tk.danger).strong());
                            for (file, reason) in &self.curves.store.problems.clone() {
                                ui.add(egui::Label::new(egui::RichText::new(format!("{}: {}", file, reason)).size(theme::TEXT_LABEL).color(tk.danger)).wrap());
                            }
                        }
                    });
                    rects.list = list.response.rect;
                    // the editor
                    let w = ui.available_width();
                    let ed = ui.allocate_ui_with_layout(egui::vec2(w, theme::CURVES_BODY_MIN_H), egui::Layout::top_down(egui::Align::Min), |ui| {
                        if let Some(old) = self.curves.opened_curve() {
                            let text = format!(
                                "You are editing the curve dated {} (FTP_Curve_Id {}). Saving over it changes the FTP of every deal priced on it and its FTP_Curve_Id. To keep it, type a new date and click \"Save as new date\".",
                                old.date.format("%Y-%m-%d"),
                                micro_parquet::curves::curve_id(old)
                            );
                            let b = ui.add(egui::Label::new(egui::RichText::new(text).size(theme::TEXT_LABEL).color(tk.warning)).wrap());
                            rects.warning = b.rect;
                            ui.add_space(theme::SPACE_S);
                        }
                        egui::Grid::new("curve_fields").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_S)).show(ui, |ui| {
                            ui.label("Date (YYYY-MM-DD):");
                            let r = ui.add(egui::TextEdit::singleline(&mut self.curves.editor.date).desired_width(theme::GAP_COMBO_NARROW_W));
                            rects.fields.push(("Date".to_string(), r.rect));
                            ui.end_row();
                            ui.label("Currency:");
                            let r = ui.add(egui::TextEdit::singleline(&mut self.curves.editor.currency).desired_width(theme::GAP_COMBO_NARROW_W));
                            rects.fields.push(("Currency".to_string(), r.rect));
                            ui.end_row();
                            ui.label("Source:");
                            let r = ui.add(egui::TextEdit::singleline(&mut self.curves.editor.source).desired_width(theme::GAP_COMBO_W * 2.0));
                            rects.fields.push(("Source".to_string(), r.rect));
                            ui.end_row();
                        });
                        ui.add_space(theme::SPACE_S);
                        ui.label(egui::RichText::new("Points (tenor in days, rate in percent; the tenors must increase)").size(theme::TEXT_LABEL).color(tk.muted));
                        let mut remove: Option<usize> = None;
                        egui::ScrollArea::vertical().id_salt("curve_points").max_height(theme::LIST_MAX_H_L).auto_shrink([false, true]).show(ui, |ui| {
                            for (i, p) in self.curves.editor.points.iter_mut().enumerate() {
                                let row = ui.horizontal(|ui| {
                                    ui.label("Tenor days:");
                                    ui.add(egui::DragValue::new(&mut p.tenor_days));
                                    ui.label("Rate %:");
                                    ui.add(egui::DragValue::new(&mut p.ftp_rate).speed(0.05).max_decimals(4)); // not a size: drag speed and decimals
                                    if ui.add(egui::Button::new(egui::RichText::new(theme::ph::X).color(tk.muted)).frame(false)).on_hover_text("Remove this point").clicked() {
                                        remove = Some(i);
                                    }
                                });
                                rects.point_rows.push(row.response.rect);
                            }
                        });
                        if let Some(i) = remove {
                            self.curves.editor.points.remove(i);
                        }
                        if ui.button(format!("{}  Add point", theme::ph::PLUS)).clicked() {
                            let next = self.curves.editor.points.last().map(|p| p.tenor_days + 1).unwrap_or(1);
                            self.curves.editor.points.push(FtpPoint { tenor_days: next, ftp_rate: 0.0 });
                        }
                        ui.add_space(theme::SPACE_S);
                        let msg = ui.scope(|ui| {
                            ui.set_max_width(ui.available_width());
                            match (&check, &self.curves.message) {
                                (Err(e), _) => {
                                    ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap());
                                }
                                (Ok(_), Some((true, m))) => {
                                    ui.add(egui::Label::new(egui::RichText::new(m).color(tk.danger)).wrap());
                                }
                                (Ok(_), Some((false, m))) => {
                                    ui.add(egui::Label::new(egui::RichText::new(m).color(tk.muted)).wrap());
                                }
                                (Ok(c), _) if self.curves.confirm_overwrite => {
                                    ui.add(egui::Label::new(egui::RichText::new(format!("Replace the stored curve of {} with these points? Its FTP_Curve_Id changes, and so does the FTP of deals priced on it.", c.date)).color(tk.warning)).wrap());
                                }
                                (Ok(c), None) => {
                                    ui.add(egui::Label::new(egui::RichText::new(format!("Ready to save as {}.", micro_parquet::curves::curve_file_name(c.date))).color(tk.muted)).wrap());
                                }
                            }
                        });
                        rects.message = msg.response.rect;
                    });
                    rects.editor = ed.response.rect;
                });
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut add = |ui: &mut egui::Ui, name: &str, enabled: bool, primary: bool, id: &'static str| {
                        let r = ui
                            .add_enabled_ui(enabled, |ui| if primary { theme::primary_button(ui, name) } else { ui.button(name) })
                            .inner;
                        rects.buttons.push((name.to_string(), r.rect));
                        if r.clicked() {
                            action = Some(id);
                        }
                    };
                    if self.curves.confirm_overwrite {
                        add(ui, "Yes, replace it", true, true, "overwrite_yes");
                        add(ui, "No", true, false, "overwrite_no");
                    } else {
                        add(ui, "Save curve", check.is_ok(), true, "save");
                        add(ui, "Save as new date", check.is_ok() && self.curves.editor.opened_from.is_some(), false, "save_new");
                    }
                    add(ui, "New curve", true, false, "new");
                    add(ui, "Import CSV\u{2026}", true, false, "import");
                    if self.curves.confirm_delete {
                        add(ui, "Yes, delete it", true, false, "delete_yes");
                        add(ui, "No", true, false, "delete_no");
                    } else {
                        add(ui, "Delete", self.curves.editor.opened_from.is_some(), false, "delete");
                    }
                    add(ui, "Close", true, false, "close");
                });
            });
        });
        self.curves.rects = rects;
        match action {
            Some("save") => {
                if let Err(e) = self.curves.save_asking() {
                    self.curves.message = Some((true, e));
                }
            }
            Some("save_new") => {
                if let Err(e) = self.curves.save_as_new_date() {
                    self.curves.message = Some((true, e));
                }
            }
            Some("overwrite_yes") => {
                self.curves.confirm_overwrite = false;
                if let Err(e) = self.curves.save() {
                    self.curves.message = Some((true, e));
                }
            }
            Some("overwrite_no") => self.curves.confirm_overwrite = false,
            Some("new") => self.curves.new_curve(),
            Some("import") => {
                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv", "txt"]).set_title("Import a curve: two columns, tenor_days and rate_pct").pick_file() {
                    match std::fs::read_to_string(&p) {
                        Ok(t) => {
                            let _ = self.curves.import_csv_text(&t);
                        }
                        Err(e) => self.curves.message = Some((true, format!("Could not read {}: {}", p.display(), e))),
                    }
                }
            }
            Some("delete") => self.curves.confirm_delete = true,
            Some("delete_no") => self.curves.confirm_delete = false,
            Some("delete_yes") => {
                self.curves.confirm_delete = false;
                if let Err(e) = self.curves.delete_opened() {
                    self.curves.message = Some((true, e));
                }
            }
            Some("close") => close = true,
            _ => {}
        }
        ModalOutcome { close, step: None }
    }
}
