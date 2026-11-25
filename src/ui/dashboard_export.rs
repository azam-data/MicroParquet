//! The dashboard's export dialog (Phase 10): HTML report, PowerPoint and PDF, from the pages as they are on the screen.

use super::dashboard::*;
use super::*;
use micro_parquet::dash_export::*;
use micro_parquet::dashboard::*;
use micro_parquet::jobs::{JobKey, JobKind};
use micro_parquet::report_html::{gate, unit_scale};
use micro_parquet::tab_jobs::{ExportOutcome, JobOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Html,
    Pptx,
    Pdf,
}

impl ExportFormat {
    fn label(self) -> &'static str {
        match self {
            ExportFormat::Html => "HTML report",
            ExportFormat::Pptx => "PowerPoint",
            ExportFormat::Pdf => "PDF",
        }
    }
    fn extension(self) -> &'static str {
        match self {
            ExportFormat::Html => "html",
            ExportFormat::Pptx => "pptx",
            ExportFormat::Pdf => "pdf",
        }
    }
}

#[derive(Default)]
pub struct ExportDlg {
    pub format: ExportFormat,
    /// "Will you email this file?": `None` until it is answered.
    pub email: Option<bool>,
    pub use_design: bool,
    /// The HTML file once it has been calculated: its size is shown before anything is written.
    pub prepared: Option<HtmlPrepared>,
    pub busy: bool,
    pub message: String,
    /// Where to write, set by a test; the app asks with a save dialog.
    pub path: Option<std::path::PathBuf>,
}

impl MicroParquetApp {
    /// The pages of the dashboard as they are on the screen, ready for a worker: sources, filters in force, places, designs.
    pub(crate) fn dash_export_src(&self) -> Result<ExportSrc, String> {
        let dash = self.project.dashboard.clone().ok_or_else(|| NO_PROJECT.to_string())?;
        let mut pages = Vec::new();
        for page in &dash.pages {
            let rects = item_rects(page).map_err(|e| format!("{:#}", e))?;
            let mut items = Vec::new();
            for (item, rect) in page.items.iter().zip(rects) {
                let res = self.dash_resolve(item).map_err(|e| format!("The item '{}' on the page '{}' can't be exported: {}", item.title_override.clone().unwrap_or_else(|| "Item".into()), page.title, e))?;
                let (picks, _) = self.dash_picks_for(page, res.data_tab.as_deref(), &item.id);
                let (scale, kpi) = match &item.kind {
                    DashItemKind::Chart(_) => (res.scale.clone().unwrap_or_else(|| unit_scale(ItemUnit::Crore)), None),
                    DashItemKind::Pivot(_) => (unit_scale(item.unit.unwrap_or_default()), None),
                    DashItemKind::Kpi(spec) => (unit_scale(ItemUnit::Crore), Some((spec.clone(), dash.default_kpi_unit))),
                };
                items.push(ExportItemSrc {
                    id: item.id.clone(),
                    title: item.title_override.clone().unwrap_or_else(|| res.title.clone()),
                    source: res.source,
                    data_tab: res.data_tab,
                    scale,
                    kpi,
                    rect,
                    picks,
                    drill: page.state.drill_of(&item.id).to_vec(),
                });
            }
            let mut values = Vec::new();
            for f in &page.filters {
                let v = self.dashboard_ui.values.get(&f.id).filter(|v| !v.sig.is_empty()).ok_or_else(|| format!("The values of the filter '{}' are not read yet. Wait a moment and try again.", f.label))?;
                values.push(v.values.clone());
            }
            let mut filter_text: Vec<String> = page.active_picks().iter().map(|(_, f, v)| format!("{} = {}", f.label, v.join(", "))).collect();
            if let Some(c) = &page.state.cross {
                filter_text.push(c.chip());
            }
            let background = match &page.background {
                Some(bg) => {
                    let copy = dash.templates.iter().find(|t| t.name == bg.name);
                    Some(micro_parquet::templates::resolve(&micro_parquet::rules::data_dir(), &bg.name, copy).map_err(|e| format!("{:#}", e))?.png)
                }
                None => None,
            };
            pages.push(ExportPageSrc { id: page.id.clone(), title: page.title.clone(), page_h: page.height(), items, filters: page.filters.clone(), values, filter_text, background });
        }
        Ok(ExportSrc { title: dash.title.clone(), pages, kpi_unit: dash.default_kpi_unit, temp_dir: self.paths.temp_dir.clone() })
    }

    pub(crate) fn dash_export_check(&mut self) {
        let src = match self.dash_export_src() {
            Ok(s) => s,
            Err(e) => {
                if let Some(d) = self.dashboard_ui.export.as_mut() {
                    d.message = e;
                }
                return;
            }
        };
        if let Some(d) = self.dashboard_ui.export.as_mut() {
            d.busy = true;
            d.prepared = None;
            d.message = "Calculating the file\u{2026}".into();
        }
        self.jobs.submit(JobKey::unique(JobKind::DashPrepare), "Preparing the HTML report", false, move |ctx| {
            ctx.set_progress("Calculating every answer of the HTML report\u{2026}", None);
            ctx.check()?;
            Ok(JobOutput::DashHtml(Box::new(prepare_html(&src)?)))
        });
    }

    pub(crate) fn apply_dash_prepared(&mut self, p: HtmlPrepared) {
        if let Some(d) = self.dashboard_ui.export.as_mut() {
            d.busy = false;
            d.message.clear();
            d.prepared = Some(p);
        }
    }

    pub(crate) fn dash_prepare_failed(&mut self, msg: &str) {
        if let Some(d) = self.dashboard_ui.export.as_mut() {
            d.busy = false;
            d.prepared = None;
            d.message = msg.to_string();
        }
    }

    fn dash_export_write(&mut self) {
        let Some(d) = self.dashboard_ui.export.as_ref() else { return };
        let (format, use_design, prepared, test_path) = (d.format, d.use_design, d.prepared.clone(), d.path.clone());
        let name = format!("{}.{}", self.project.dashboard.as_ref().map(|x| x.title.clone()).unwrap_or_else(|| "Dashboard".into()), format.extension());
        let path = match test_path {
            Some(p) => p,
            None => {
                let dlg = rfd::FileDialog::new().add_filter(format.label(), &[format.extension()]).set_file_name(&name).set_title("Export the dashboard");
                match dlg.save_file() {
                    Some(p) => p,
                    None => return,
                }
            }
        };
        let src = match format {
            ExportFormat::Html => None,
            _ => match self.dash_export_src() {
                Ok(s) => Some(s),
                Err(e) => {
                    if let Some(d) = self.dashboard_ui.export.as_mut() {
                        d.message = e;
                    }
                    return;
                }
            },
        };
        let label = format!("Writing the {}", format.label());
        self.jobs.submit(JobKey::unique(JobKind::Export), label, true, move |ctx| {
            ctx.check()?;
            let message = match (format, src, prepared) {
                (ExportFormat::Html, _, Some(p)) => write_html(&path, &p)?,
                (ExportFormat::Html, _, None) => anyhow::bail!("Check the size first."),
                (ExportFormat::Pptx, Some(s), _) => export_pptx(&s, use_design, &path)?,
                (ExportFormat::Pdf, Some(s), _) => export_pdf(&s, &path)?.0,
                _ => anyhow::bail!("Nothing to export."),
            };
            Ok(JobOutput::Exported(ExportOutcome { message, path_used: None, written: Some(path) }))
        });
        if let Some(d) = self.dashboard_ui.export.as_mut() {
            d.message = "Writing the file\u{2026} the result is in the status bar.".into();
        }
    }

    /// The Export dialog, over the dashboard.
    pub(crate) fn render_export_dialog(&mut self, ctx: &egui::Context, rects: &mut Vec<(String, egui::Rect)>) {
        if self.dashboard_ui.export.is_none() {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let any_design = self.project.dashboard.as_ref().is_some_and(|d| d.pages.iter().any(|p| p.background.is_some()));
        let mut close = false;
        let mut check = false;
        let mut write = false;
        let mut dlg = self.dashboard_ui.export.take().unwrap_or_default();
        theme::modal("Export the dashboard").show(ctx, |d| {
            let ui = &mut *d.ui;
            ui.horizontal(|ui| {
                for f in [ExportFormat::Html, ExportFormat::Pptx, ExportFormat::Pdf] {
                    let r = ui.selectable_label(dlg.format == f, f.label());
                    rects.push((format!("Export format {}", f.label()), r.rect));
                    if r.clicked() && dlg.format != f {
                        dlg.format = f;
                        dlg.message.clear();
                    }
                }
            });
            ui.add_space(theme::SPACE_S);
            match dlg.format {
                ExportFormat::Html => {
                    ui.label("One file that works offline: pages as tabs, filters, clicks, drill.");
                    ui.label(theme::semibold("Will you email this file?"));
                    ui.horizontal(|ui| {
                        for (name, v) in [("Yes", true), ("No", false)] {
                            let r = ui.selectable_label(dlg.email == Some(v), name);
                            rects.push((format!("Export email {}", name), r.rect));
                            if r.clicked() {
                                dlg.email = Some(v);
                            }
                        }
                    });
                    if dlg.email.is_none() {
                        ui.label(egui::RichText::new("Answer first: over 5 MB an email may not take the file.").color(tk.muted));
                    }
                    let can_check = dlg.email.is_some() && !dlg.busy;
                    let r = ui.add_enabled(can_check, egui::Button::new("Check size"));
                    rects.push(("Export check size".into(), r.rect));
                    if r.clicked() {
                        check = true;
                    }
                    if let (Some(p), Some(email)) = (&dlg.prepared, dlg.email) {
                        let g = gate(email, &p.parts, p.has_drill, p.pages);
                        ui.label(&g.message);
                        for s in &g.suggestions {
                            ui.label(format!("\u{2022} {}", s));
                        }
                        if !p.not_linked.is_empty() {
                            ui.label(theme::semibold("Not linked to clicks (they still follow the filters):"));
                            for n in &p.not_linked {
                                ui.label(egui::RichText::new(n.clone()).color(tk.muted));
                            }
                        }
                        let r = ui.add_enabled(g.allowed && !dlg.busy, egui::Button::new("Export\u{2026}"));
                        rects.push(("Export write".into(), r.rect));
                        if r.clicked() {
                            write = true;
                        }
                    }
                }
                ExportFormat::Pptx => {
                    ui.label("One slide per item, with editable charts and tables.");
                    if any_design {
                        let r = ui.checkbox(&mut dlg.use_design, "Use design as slide background");
                        rects.push(("Export use design".into(), r.rect));
                    }
                    let r = ui.add_enabled(!dlg.busy, egui::Button::new("Export\u{2026}"));
                    rects.push(("Export write".into(), r.rect));
                    write = r.clicked();
                }
                ExportFormat::Pdf => {
                    ui.label("One A4 landscape page per dashboard page, items where they are on the page.");
                    let r = ui.add_enabled(!dlg.busy, egui::Button::new("Export\u{2026}"));
                    rects.push(("Export write".into(), r.rect));
                    write = r.clicked();
                }
            }
            if !dlg.message.is_empty() {
                ui.colored_label(tk.muted, dlg.message.clone());
            }
            ui.add_space(theme::SPACE_S);
            let r = ui.button("Close");
            rects.push(("Export close".into(), r.rect));
            close = r.clicked();
        });
        self.dashboard_ui.export = if close { None } else { Some(dlg) };
        if check {
            self.dash_export_check();
        }
        if write {
            self.dash_export_write();
        }
    }
}
