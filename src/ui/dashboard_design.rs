//! Design templates on the dashboard (Phase 10 D5): importing a picture, drawing and naming its slots, saving it, using it as a page's
//! background and filling its slots with items. The files, the hash and the project copy are `micro_parquet::templates`.

use crate::ui::dashboard::TplEdit;
use crate::ui::prelude::*;
use micro_parquet::dashboard::*;
use micro_parquet::templates::{self, Design};

/// The picture as a texture.
pub(crate) fn texture_of(ctx: &egui::Context, name: &str, png: &[u8]) -> anyhow::Result<egui::TextureHandle> {
    let img = image::load_from_memory(png).map_err(|e| anyhow::anyhow!("The design picture could not be read: {}", e))?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR))
}

fn button(ui: &mut egui::Ui, name: &str, text: &str, rects: &mut Vec<(String, egui::Rect)>) -> bool {
    let r = ui.button(text);
    rects.push((name.to_string(), r.rect));
    r.clicked()
}

/// The page-sized area that fits the space left, and its scale (points per page unit).
fn fit_area(ui: &mut egui::Ui, page_h: f64) -> (egui::Rect, f32) {
    let avail = ui.available_rect_before_wrap().shrink2(egui::vec2(theme::SPACE_S, 0.0));
    let scale = (avail.width() / PAGE_W as f32).min(avail.height() / page_h as f32).max(theme::DASH_MIN_SCALE);
    let (area, _) = ui.allocate_exact_size(egui::vec2(PAGE_W as f32 * scale, page_h as f32 * scale), egui::Sense::hover());
    (area.translate(egui::vec2(theme::SPACE_S, 0.0)), scale)
}

fn to_units(area: egui::Rect, scale: f32, p: egui::Pos2) -> (f64, f64) {
    (((p.x - area.min.x) / scale) as f64, ((p.y - area.min.y) / scale) as f64)
}

impl MicroParquetApp {
    /// Reads the picture the Import button picked and opens the design editor on it.
    pub(crate) fn dash_load_import(&mut self, ctx: &egui::Context) {
        let Some(path) = self.dashboard_ui.import_path.take() else { return };
        let made = templates::design_from_file(&path).and_then(|d| texture_of(ctx, &format!("design-{}", d.blake3), &d.png).map(|t| (d, t)));
        match made {
            Ok((design, texture)) => {
                self.dashboard_ui.notice = design.note.clone().unwrap_or_default();
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.dashboard_ui.tpl = Some(TplEdit { name, design, texture, slots: Vec::new(), drawing: None, error: String::new() });
            }
            Err(e) => self.dashboard_ui.notice = format!("{:#}", e),
        }
    }

    /// Makes the textures of the backgrounds the dashboard's pages use, once each; a design that cannot be read says why.
    pub(crate) fn dash_load_backgrounds(&mut self, ctx: &egui::Context, dash: &Dashboard) {
        for page in &dash.pages {
            let Some(bg) = &page.background else { continue };
            if self.dashboard_ui.textures.contains_key(&bg.blake3) || self.dashboard_ui.design_notes.get(&bg.name).is_some_and(|n| n.starts_with("!")) {
                continue;
            }
            let copy = dash.templates.iter().find(|t| t.name == bg.name);
            match templates::resolve(&micro_parquet::rules::data_dir(), &bg.name, copy).and_then(|r| texture_of(ctx, &format!("bg-{}", r.blake3), &r.png).map(|t| (r, t))) {
                Ok((r, tex)) => {
                    if let Some(w) = r.warning {
                        self.dashboard_ui.design_notes.insert(bg.name.clone(), w);
                    }
                    self.dashboard_ui.textures.insert(bg.blake3.clone(), tex);
                }
                // a leading "!" marks a design that is not tried again every frame
                Err(e) => {
                    self.dashboard_ui.design_notes.insert(bg.name.clone(), format!("!{:#}", e));
                }
            }
        }
    }

    /// What to say about the design of a page (a hash mismatch, a missing or damaged picture).
    pub(crate) fn dash_design_note(&self, page: &DashPage) -> Option<String> {
        let bg = page.background.as_ref()?;
        self.dashboard_ui.design_notes.get(&bg.name).map(|n| n.trim_start_matches('!').to_string())
    }

    /// Uses the design `name` for the page (`None` takes the design away): its picture becomes the background and its slots empty places.
    pub(crate) fn dash_apply_design(&mut self, dash: &mut Dashboard, pi: usize, name: Option<&str>) {
        let Some(name) = name else {
            let page = &mut dash.pages[pi];
            page.background = None;
            page.slots.clear();
            page.set_height(PAGE_H);
            page.items.iter_mut().for_each(|i| i.slot = None);
            return;
        };
        let copy = dash.templates.iter().find(|t| t.name == name).cloned();
        let r = match templates::resolve(&micro_parquet::rules::data_dir(), name, copy.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                self.dashboard_ui.notice = format!("{:#}", e);
                return;
            }
        };
        if let Some(w) = &r.warning {
            self.dashboard_ui.design_notes.insert(name.to_string(), w.clone());
        } else {
            self.dashboard_ui.design_notes.remove(name);
        }
        let tref = TemplateRef { name: name.to_string(), blake3: r.blake3.clone() };
        if copy.is_none() {
            let (w, h) = image::load_from_memory(&r.png).map(|i| (i.width(), i.height())).unwrap_or((0, 0));
            dash.templates.push(TemplateData { name: name.to_string(), blake3: r.blake3.clone(), w, h, png_b64: templates::base64_encode(&r.png), slots: r.slots.clone() });
        }
        if !dash.template_library.iter().any(|t| t.name == name) {
            dash.template_library.push(tref.clone());
        }
        match dash.pages[pi].apply_template(tref, r.slots, page_height_for(r.w, r.h)) {
            Ok(()) => self.dashboard_ui.notice.clear(),
            Err(e) => self.dashboard_ui.notice = format!("{:#}", e),
        }
    }

    /// The "Design" part of the page's settings: the design the page uses, Import, and what is left to fill.
    pub(crate) fn design_settings(&mut self, ui: &mut egui::Ui, dash: &mut Dashboard, pi: usize, rects: &mut Vec<(String, egui::Rect)>) {
        let tk = *theme::tokens(self.dark_mode);
        ui.label(theme::semibold("Design"));
        let mut names: Vec<String> = dash.template_library.iter().map(|t| t.name.clone()).collect();
        for n in templates::list_folder(&micro_parquet::rules::data_dir()) {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        let current = dash.pages[pi].background.as_ref().map(|b| b.name.clone());
        let mut pick: Option<Option<String>> = None;
        let r = egui::ComboBox::from_id_salt("dash_design").selected_text(current.clone().unwrap_or_else(|| "None".to_string())).width(theme::DASH_RIGHT_W - theme::DASH_PROPS_PAD).show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), "None").clicked() {
                pick = Some(None);
            }
            for n in &names {
                if ui.selectable_label(current.as_deref() == Some(n.as_str()), n).clicked() {
                    pick = Some(Some(n.clone()));
                }
            }
        });
        rects.push(("Page design".into(), r.response.rect));
        if let Some(p) = pick {
            self.dash_apply_design(dash, pi, p.as_deref());
        }
        if button(ui, "Import design", "Import design\u{2026}", rects) {
            if let Some(p) = rfd::FileDialog::new().add_filter("Picture", &["png", "jpg", "jpeg"]).pick_file() {
                self.dashboard_ui.import_path = Some(p);
            }
        }
        if let Some(n) = self.dash_design_note(&dash.pages[pi]) {
            ui.label(egui::RichText::new(n).color(tk.warning));
        }
        let page = &dash.pages[pi];
        if !page.slots.is_empty() {
            ui.label(egui::RichText::new(format!("{} of {} slots are empty. Click an empty slot on the page to fill it.", page.empty_slots().len(), page.slots.len())).color(tk.muted));
        }
    }

    /// The choices of the slot being filled, under the header.
    pub(crate) fn fill_row(&mut self, ui: &mut egui::Ui, dash: &mut Dashboard, pi: usize, choices: &[(String, String, bool)], rects: &mut Vec<(String, egui::Rect)>) {
        let Some(name) = self.dashboard_ui.fill_slot.clone() else { return };
        let Some(slot) = dash.pages[pi].slots.iter().find(|s| s.name == name).cloned() else {
            self.dashboard_ui.fill_slot = None;
            return;
        };
        let mut chosen: Option<DashItemKind> = None;
        let mut cancel = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Fill '{}' ({}) with:", slot.name, slot.kind.label().to_lowercase()));
            if slot.kind.accepts(&DashItemKind::Kpi(Default::default())) && button(ui, "Fill with KPI card", "KPI card", rects) {
                chosen = Some(DashItemKind::Kpi(micro_parquet::kpi::KpiSpec { title: "New KPI".into(), ..Default::default() }));
            }
            for (key, title, chart) in choices {
                let kind = if *chart { DashItemKind::Chart(key.clone()) } else { DashItemKind::Pivot(key.clone()) };
                if slot.kind.accepts(&kind) && button(ui, &format!("Fill with {}", title), title, rects) {
                    chosen = Some(kind);
                }
            }
            if button(ui, "Cancel fill", "Cancel", rects) {
                cancel = true;
            }
        });
        if let Some(kind) = chosen {
            let taken: Vec<String> = dash.pages[pi].items.iter().map(|i| i.id.clone()).collect();
            let id = self.dashboard_ui.fresh_id("i", &taken);
            match dash.pages[pi].fill_slot(&name, DashItem { id: id.clone(), kind, ..Default::default() }) {
                Ok(()) => {
                    self.dashboard_ui.selected = Some(id);
                    self.dashboard_ui.notice.clear();
                }
                Err(e) => self.dashboard_ui.notice = format!("{:#}", e),
            }
            self.dashboard_ui.fill_slot = None;
        }
        if cancel {
            self.dashboard_ui.fill_slot = None;
        }
    }

    /// The editor of a design just imported: drag on the picture to draw a slot, name it, give it a type, then save.
    pub(crate) fn render_design_editor(&mut self, ui: &mut egui::Ui, rects: &mut Vec<(String, egui::Rect)>) {
        let tk = *theme::tokens(self.dark_mode);
        let Some(mut ed) = self.dashboard_ui.tpl.take() else { return };
        let (mut save, mut cancel) = (false, false);
        ui.horizontal(|ui| {
            ui.label(theme::semibold(format!("{}  New design", theme::ph::CHART_BAR)).size(theme::TEXT_HEADING));
            ui.label(egui::RichText::new("Drag on the picture to draw a slot.").color(tk.muted));
        });
        if !self.dashboard_ui.notice.is_empty() {
            ui.colored_label(tk.warning, self.dashboard_ui.notice.clone());
        }
        if !ed.error.is_empty() {
            ui.colored_label(tk.danger, ed.error.clone());
        }
        egui::SidePanel::right("dash_design_props").exact_width(theme::DASH_RIGHT_W).resizable(false).frame(egui::Frame::none()).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("dash_design_scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.label(theme::semibold("Design name"));
                let r = ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(theme::DASH_RIGHT_W - theme::DASH_PROPS_PAD));
                rects.push(("Design name".into(), r.rect));
                ui.label(theme::semibold(format!("Slots ({})", ed.slots.len())));
                let mut remove: Option<usize> = None;
                for i in 0..ed.slots.len() {
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut ed.slots[i].name).desired_width(theme::DASH_SLOT_NAME_W));
                        rects.push((format!("Slot {} name", i + 1), r.rect));
                        let mut kind = ed.slots[i].kind;
                        let c = egui::ComboBox::from_id_salt(("dash_slot_kind", i)).selected_text(kind.label()).width(theme::DASH_PER_ROW_W).show_ui(ui, |ui| {
                            for k in SlotKind::ALL {
                                ui.selectable_value(&mut kind, k, k.label());
                            }
                        });
                        rects.push((format!("Slot {} type", i + 1), c.response.rect));
                        ed.slots[i].kind = kind;
                        if button(ui, &format!("Remove slot {}", i + 1), theme::ph::X, rects) {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    ed.slots.remove(i);
                }
                ui.add_space(theme::SPACE_S);
                ui.horizontal(|ui| {
                    save = button(ui, "Save design", "Save design", rects);
                    cancel = button(ui, "Cancel design", "Cancel", rects);
                });
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::none()).show_inside(ui, |ui| {
            let page_h = ed.design.page_h();
            let (area, scale) = fit_area(ui, page_h);
            ui.painter().image(ed.texture.id(), area, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            ui.painter().rect_stroke(area, theme::RADIUS_PANEL, theme::line(tk.border));
            rects.push(("Design area".into(), area));
            let resp = ui.interact(area, ui.id().with("design_area"), egui::Sense::drag());
            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() {
                    ed.drawing = Some((p, p));
                }
            }
            if let (true, Some((a, _)), Some(p)) = (resp.dragged(), ed.drawing, resp.interact_pointer_pos()) {
                ed.drawing = Some((a, p));
            }
            if resp.drag_stopped() {
                if let Some((a, b)) = ed.drawing.take() {
                    let (x0, y0) = to_units(area, scale, a);
                    let (x1, y1) = to_units(area, scale, b);
                    let l = snap(x0.min(x1)).clamp(0.0, PAGE_W);
                    let t = snap(y0.min(y1)).clamp(0.0, page_h);
                    let r = snap(x0.max(x1)).clamp(0.0, PAGE_W);
                    let bt = snap(y0.max(y1)).clamp(0.0, page_h);
                    let rect = DashRect { x: l, y: t, w: r - l, h: bt - t };
                    if rect.w >= MIN_ITEM_W && rect.h >= MIN_ITEM_H {
                        let mut n = ed.slots.len() + 1;
                        while ed.slots.iter().any(|s| s.name == format!("Slot {n}")) {
                            n += 1;
                        }
                        ed.slots.push(Slot { name: format!("Slot {n}"), kind: SlotKind::Any, rect });
                        ed.error.clear();
                    } else {
                        ed.error = format!("That slot is too small. Draw it at least {} \u{d7} {} units.", MIN_ITEM_W, MIN_ITEM_H);
                    }
                }
            }
            let to_screen = |r: &DashRect| egui::Rect::from_min_size(area.min + egui::vec2(r.x as f32 * scale, r.y as f32 * scale), egui::vec2(r.w as f32 * scale, r.h as f32 * scale));
            for (i, s) in ed.slots.iter().enumerate() {
                let sr = to_screen(&s.rect);
                ui.painter().rect(sr, 0.0, tk.accent.gamma_multiply(theme::DASH_SLOT_FILL), theme::line_thick(tk.accent));
                ui.painter().text(sr.left_top() + egui::vec2(theme::SPACE_XS, theme::SPACE_XS), egui::Align2::LEFT_TOP, format!("{} \u{b7} {}", s.name, s.kind.label()), egui::FontId::proportional(theme::TEXT_LABEL), tk.ink);
                rects.push((format!("Slot {} rect", i + 1), sr));
            }
            if let Some((a, b)) = ed.drawing {
                ui.painter().rect_stroke(egui::Rect::from_two_pos(a, b), 0.0, theme::line_thick(tk.accent));
            }
        });
        let mut keep = true;
        if cancel {
            keep = false;
        }
        if save {
            match self.save_design(&ed) {
                Ok(()) => {
                    self.dashboard_ui.notice = format!("The design '{}' is saved. Pick it under Design in the page settings.", ed.name.trim());
                    keep = false;
                }
                Err(e) => ed.error = format!("{:#}", e),
            }
        }
        if keep {
            self.dashboard_ui.tpl = Some(ed);
        }
    }

    /// Saves the design in the data folder and a copy in the project.
    fn save_design(&mut self, ed: &TplEdit) -> anyhow::Result<()> {
        let name = ed.name.trim().to_string();
        let Some(dash) = self.project.dashboard.as_mut() else { anyhow::bail!("Dashboards belong to a project. Save or open a project first.") };
        templates::save_to_folder(&micro_parquet::rules::data_dir(), &name, &ed.design, &ed.slots)?;
        let copy = templates::project_copy(&name, &ed.design, &ed.slots);
        let tref = TemplateRef { name: name.clone(), blake3: ed.design.blake3.clone() };
        dash.templates.retain(|t| t.name != name);
        dash.templates.push(copy);
        dash.template_library.retain(|t| t.name != name);
        dash.template_library.push(tref);
        Ok(())
    }
}

#[allow(dead_code)]
fn _design_is_used(_: &Design) {}
