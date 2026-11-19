//! Report packs (Phase 6 P2): Banking ▸ "Report packs". Build a pack from open tabs (input slots,
//! items, output), save it in `portable_data/packs`, and run it on this month's files.

use crate::ui::prelude::*;
use micro_parquet::packs::*;
use micro_parquet::tool_runs::ToolRun;
use std::collections::BTreeMap;

/// What the dialog drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct PacksRects {
    pub fields: Vec<(String, egui::Rect)>,
    pub buttons: Vec<(String, egui::Rect)>,
    pub rows: Vec<(String, egui::Rect)>,
    pub message: Option<egui::Rect>,
}

#[derive(Default)]
pub struct PacksState {
    /// The pack being built.
    pub pack: ReportPack,
    /// The name of the saved pack this was opened from ("" for a new one).
    pub opened_from: String,
    pub saved: Vec<String>,
    /// The file typed for each slot of the run.
    pub files: BTreeMap<String, String>,
    pub cutoff: String,
    /// Where the workbook goes ("" = next to the first slot's file).
    pub out_path: String,
    /// (is an error, text)
    pub message: Option<(bool, String)>,
    /// Saving over another pack: asked once.
    pub confirm_replace: bool,
    pub rects: PacksRects,
}

/// A sheet name for an item: the title without what Excel refuses, at most 28 characters, not used yet.
pub fn sheet_for(title: &str, taken: &[String]) -> String {
    let base: String = title.chars().map(|c| if matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') { '_' } else { c }).take(28).collect::<String>().trim().to_string();
    let base = if base.is_empty() { "Sheet".to_string() } else { base };
    let mut name = base.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) || name.eq_ignore_ascii_case(COVER_SHEET) {
        let suffix = format!(" {}", n);
        name = format!("{}{}", base.chars().take(28 - suffix.chars().count().min(27)).collect::<String>().trim_end(), suffix);
        n += 1;
    }
    name
}

impl MicroParquetApp {
    pub(crate) fn open_packs(&mut self) {
        self.packs.saved = list_packs(&packs_dir());
        if self.packs.cutoff.trim().is_empty() {
            self.packs.cutoff = crate::ui::cashflow_ladder::last_month_end(chrono::Local::now().date_naive());
        }
        self.packs.message = None;
        self.sync_pack_files();
        self.active_tool = ActiveToolModal::Packs;
    }

    /// One file box for each slot (kept when the slot keeps its name).
    fn sync_pack_files(&mut self) {
        let slots: Vec<String> = self.packs.pack.inputs.iter().map(|i| i.slot.clone()).collect();
        self.packs.files.retain(|k, _| slots.contains(k));
        for s in slots {
            self.packs.files.entry(s).or_default();
        }
    }

    fn pack_message(&mut self, error: bool, text: impl Into<String>) {
        self.packs.message = Some((error, text.into()));
    }

    /// "Add input slot from open tab": the tab's file, schema and steps.
    pub(crate) fn pack_add_slot_from_tab(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return self.pack_message(true, "Open the tab the slot is made from first.");
        };
        let TabSource::SingleFile { path, config, .. } = &tab.source else {
            return self.pack_message(true, "A slot is made from a tab opened from a file. This tab is a result: add it as an item instead.");
        };
        let mut required: Vec<String> = config.columns.iter().map(|c| c.name.clone()).collect();
        if required.is_empty() {
            match detect_file_schema(path) {
                Ok(d) => required = d.columns.into_iter().map(|c| c.name).collect(),
                Err(e) => return self.pack_message(true, format!("The file could not be read: {:#}", e)),
            }
        }
        let taken: Vec<String> = self.packs.pack.inputs.iter().map(|i| i.slot.clone()).collect();
        let mut slot = tab.title.clone();
        let mut n = 2;
        while taken.iter().any(|t| t.eq_ignore_ascii_case(&slot)) {
            slot = format!("{} {}", tab.title, n);
            n += 1;
        }
        let exec = tab.exec_steps.min(tab.steps.len());
        let input = PackInput { slot: slot.clone(), schema_template: None, schema: Some(config.clone()), steps: tab.steps[..exec].to_vec(), required_columns: required, captured_from: tab.title.clone() };
        self.packs.pack.inputs.push(input);
        self.sync_pack_files();
        self.pack_message(false, format!("Added the slot '{}': {} column(s) and {} step(s) of the tab.", slot, self.packs.pack.inputs.last().map(|i| i.required_columns.len()).unwrap_or(0), exec));
    }

    /// "Add item from result tab": the active tab's tool run (or pivot), its source tabs turned into slots.
    pub(crate) fn pack_add_item_from_tab(&mut self) {
        let idx = self.active_tab;
        let run = match self.tab_run(idx) {
            Ok(r) => r,
            Err(e) => return self.pack_message(true, format!("This tab cannot be an item: {}", e)),
        };
        let slots: Vec<(String, String)> = self.packs.pack.inputs.iter().map(|i| (i.slot.clone(), i.captured_from.clone())).collect();
        let mut run = run;
        let mut missing: Vec<String> = Vec::new();
        run.each_source_name(&mut |n| match slots.iter().find(|(s, c)| s == n || c == n) {
            Some((s, _)) => *n = s.clone(),
            None => missing.push(n.clone()),
        });
        if !missing.is_empty() {
            missing.dedup();
            return self.pack_message(true, format!("Add the input slot from the open tab '{}' first: the item reads it.", missing.join("', '")));
        }
        let (title, col_formats) = self.tabs.get(idx).map(|t| (t.title.clone(), t.col_formats.clone())).unwrap_or_default();
        let source = match run {
            ToolRun::PivotTab(p) => PackItemKind::Pivot { slot: p.source, pivot: p.pivot },
            other => PackItemKind::Run { run: other },
        };
        let taken: Vec<String> = self.packs.pack.items.iter().map(|i| i.sheet.clone()).collect();
        let sheet = sheet_for(&title, &taken);
        self.packs.pack.items.push(PackItem { title: title.clone(), sheet, source, col_formats });
        self.pack_message(false, format!("Added the item '{}'.", title));
    }

    fn pack_add_slot_table(&mut self, slot: &str) {
        let taken: Vec<String> = self.packs.pack.items.iter().map(|i| i.sheet.clone()).collect();
        let sheet = sheet_for(slot, &taken);
        self.packs.pack.items.push(PackItem { title: format!("{} table", slot), sheet, source: PackItemKind::Steps { slot: slot.to_string() }, col_formats: Default::default() });
    }

    fn pack_remove_slot(&mut self, i: usize) {
        let Some(slot) = self.packs.pack.inputs.get(i).map(|s| s.slot.clone()) else {
            return;
        };
        if let Some(item) = self.packs.pack.items.iter().find(|it| item_slots(it).iter().any(|s| s.eq_ignore_ascii_case(&slot))) {
            let t = item.title.clone();
            return self.pack_message(true, format!("The slot '{}' is read by the item '{}'. Remove the item first.", slot, t));
        }
        self.packs.pack.inputs.remove(i);
        self.sync_pack_files();
    }

    fn pack_save(&mut self) {
        let name = self.packs.pack.name.trim().to_string();
        if let Err(e) = validate_pack(&self.packs.pack) {
            return self.pack_message(true, format!("{:#}", e));
        }
        let exists = pack_path(&packs_dir(), &name).map(|p| p.exists()).unwrap_or(false);
        if exists && name != self.packs.opened_from && !self.packs.confirm_replace {
            self.packs.confirm_replace = true;
            return self.pack_message(true, format!("A pack called '{}' exists. Choose Replace to overwrite it, or type another name.", name));
        }
        match save_pack(&packs_dir(), &self.packs.pack) {
            Ok(p) => {
                self.packs.opened_from = name.clone();
                self.packs.confirm_replace = false;
                self.packs.saved = list_packs(&packs_dir());
                self.pack_message(false, format!("Saved the pack as {}.", p.display()));
                self.status_message = format!("Saved the pack '{}'.", name);
            }
            Err(e) => self.pack_message(true, format!("{:#}", e)),
        }
    }

    #[cfg(test)]
    pub(crate) fn pack_open_for_test(&mut self, name: &str) {
        self.pack_open(name);
    }

    fn pack_open(&mut self, name: &str) {
        match load_pack(&packs_dir(), name) {
            Ok(p) => {
                self.packs.opened_from = p.name.clone();
                self.packs.pack = p;
                self.packs.confirm_replace = false;
                self.sync_pack_files();
                self.pack_message(false, format!("Opened the pack '{}'.", name));
            }
            Err(e) => self.pack_message(true, format!("{:#}", e)),
        }
    }

    /// Runs the pack on the typed files on the worker; the workbook is written only when every item worked.
    pub(crate) fn pack_run(&mut self) {
        let pack = self.packs.pack.clone();
        if let Err(e) = validate_pack(&pack) {
            return self.pack_message(true, format!("{:#}", e));
        }
        let mut files = BTreeMap::new();
        for (slot, f) in &self.packs.files {
            if !f.trim().is_empty() {
                files.insert(slot.clone(), PathBuf::from(f.trim()));
            }
        }
        let cutoff = self.packs.cutoff.trim().to_string();
        let out = if self.packs.out_path.trim().is_empty() {
            let dir = files.values().next().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| self.paths.root_dir.clone());
            dir.join(format!("{}_{}.xlsx", micro_parquet::paths::sanitize_file_name(&pack.name), cutoff))
        } else {
            PathBuf::from(self.packs.out_path.trim())
        };
        let pdf = if pack.output.pdf { Some(out.with_extension("pdf")) } else { None };
        let spec = PackRunSpec { files, cutoff };
        let paths = self.paths.clone();
        let label = format!("Running pack '{}'", pack.name);
        self.jobs.submit(JobKey::unique(JobKind::Export), label, true, move |ctx| {
            let r = run_pack(&pack, &spec, &paths, &run_time_now(), ctx)?;
            let message = write_pack(&r, &out, pdf.as_deref(), ctx)?;
            Ok(JobOutput::Exported(micro_parquet::tab_jobs::ExportOutcome { message, path_used: None, written: Some(out) }))
        });
        self.pack_message(false, "The pack is running; the status bar shows its progress. The workbook is written when every item has worked.");
        self.status_message = "Running the pack\u{2026}".to_string();
    }

    pub(crate) fn render_packs_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects = PacksRects::default();
        let mut close = false;
        let title = format!("{}  Report packs", theme::ph::TABLE);
        #[derive(Clone)]
        enum Act {
            AddSlot,
            AddItem,
            SlotTable(String),
            RemoveSlot(usize),
            RemoveItem(usize),
            Up(usize),
            Down(usize),
            Save,
            Open(String),
            New,
            Run,
            Choose(String),
            ChooseOut,
        }
        let mut act: Option<Act> = None;
        let name_w = theme::FIELD_W_SORT_LIST * theme::HALF;
        theme::modal(&title).default_width(theme::CURVES_DIALOG_W).fill_height().show(ctx, |d| {
            d.header(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Name:");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.packs.pack.name).desired_width(theme::FIELD_W_SORT_LIST));
                    rects.fields.push(("Pack name".to_string(), r.rect));
                    if r.changed() {
                        self.packs.confirm_replace = false;
                    }
                    let label = if self.packs.confirm_replace { "Replace" } else { "Save pack" };
                    let b = ui.button(label);
                    rects.buttons.push((label.to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::Save);
                    }
                    let b = ui.button("New pack");
                    rects.buttons.push(("New pack".to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::New);
                    }
                    let saved = self.packs.saved.clone();
                    let r = egui::ComboBox::from_id_salt("packs_open").selected_text("Open a saved pack").show_ui(ui, |ui| {
                        for n in &saved {
                            if ui.selectable_label(*n == self.packs.opened_from, n).clicked() {
                                act = Some(Act::Open(n.clone()));
                            }
                        }
                    });
                    rects.buttons.push(("Open a saved pack".to_string(), r.response.rect));
                });
            });
            d.body_scroll(|ui| {
                // ---- the input slots
                theme::card(ui, "Input slots", |ui| {
                    if self.packs.pack.inputs.is_empty() {
                        ui.label(egui::RichText::new("No slot yet. A slot is a file the pack is given each month (Loans, Deposits). Open the file in a tab, then add it.").color(tk.muted));
                    }
                    for i in 0..self.packs.pack.inputs.len() {
                        let row = ui.horizontal(|ui| {
                            let inp = &mut self.packs.pack.inputs[i];
                            let r = ui.add(egui::TextEdit::singleline(&mut inp.slot).desired_width(name_w));
                            rects.fields.push((format!("Slot {}", i + 1), r.rect));
                            let schema = match (&inp.schema_template, &inp.schema) {
                                (Some(t), _) => format!("schema '{}'", t),
                                (None, Some(_)) => "schema of the tab".to_string(),
                                _ => "detected schema".to_string(),
                            };
                            ui.add(egui::Label::new(egui::RichText::new(format!("{} \u{b7} {} step(s) \u{b7} {} column(s) required", schema, inp.steps.len(), inp.required_columns.len())).color(tk.muted)).truncate());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let b = ui.button("Remove");
                                rects.buttons.push((format!("Remove slot {}", i + 1), b.rect));
                                if b.clicked() {
                                    act = Some(Act::RemoveSlot(i));
                                }
                                let slot = inp.slot.clone();
                                let b = ui.button("Add its table as an item");
                                rects.buttons.push((format!("Add table of slot {}", i + 1), b.rect));
                                if b.clicked() {
                                    act = Some(Act::SlotTable(slot));
                                }
                            });
                        });
                        rects.rows.push((format!("Slot {}", i + 1), row.response.rect));
                    }
                    let b = ui.button("Add input slot from open tab");
                    rects.buttons.push(("Add input slot from open tab".to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::AddSlot);
                    }
                });
                // ---- the items
                theme::card(ui, "Items (one sheet each, after the Cover)", |ui| {
                    if self.packs.pack.items.is_empty() {
                        ui.label(egui::RichText::new("No item yet. Open the result tab of a tool (or a pivot) and add it.").color(tk.muted));
                    }
                    let n = self.packs.pack.items.len();
                    for i in 0..n {
                        let row = ui.horizontal(|ui| {
                            let b = ui.add_enabled(i > 0, egui::Button::new(theme::ph::ARROW_UP));
                            rects.buttons.push((format!("Up {}", i + 1), b.rect));
                            if b.clicked() {
                                act = Some(Act::Up(i));
                            }
                            let b = ui.add_enabled(i + 1 < n, egui::Button::new(theme::ph::ARROW_DOWN));
                            rects.buttons.push((format!("Down {}", i + 1), b.rect));
                            if b.clicked() {
                                act = Some(Act::Down(i));
                            }
                            let it = &mut self.packs.pack.items[i];
                            let r = ui.add(egui::TextEdit::singleline(&mut it.title).desired_width(name_w * 2.0));
                            rects.fields.push((format!("Item title {}", i + 1), r.rect));
                            let r = ui.add(egui::TextEdit::singleline(&mut it.sheet).desired_width(name_w));
                            rects.fields.push((format!("Sheet {}", i + 1), r.rect));
                            let what = match &it.source {
                                PackItemKind::Steps { slot } => format!("table of {}", slot),
                                PackItemKind::Pivot { slot, .. } => format!("pivot on {}", slot),
                                PackItemKind::Run { run } => format!("{} on {}", run.tool_name(), run.source_names().join(", ")),
                            };
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let b = ui.button("Remove");
                                rects.buttons.push((format!("Remove item {}", i + 1), b.rect));
                                if b.clicked() {
                                    act = Some(Act::RemoveItem(i));
                                }
                                ui.add(egui::Label::new(egui::RichText::new(what).color(tk.muted)).truncate());
                            });
                        });
                        rects.rows.push((format!("Item {}", i + 1), row.response.rect));
                    }
                    let b = ui.button("Add item from result tab");
                    rects.buttons.push(("Add item from result tab".to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::AddItem);
                    }
                });
                // ---- the output
                theme::card(ui, "Output (Excel is always written)", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let r = ui.checkbox(&mut self.packs.pack.output.pdf, "Also write a PDF");
                        rects.fields.push(("PDF".to_string(), r.rect));
                        ui.label("Money in");
                        let cur = self.packs.pack.output.scale;
                        let r = egui::ComboBox::from_id_salt("pack_scale").selected_text(cur.label()).show_ui(ui, |ui| {
                            for f in NumberDisplayFormat::all() {
                                ui.selectable_value(&mut self.packs.pack.output.scale, *f, f.label());
                            }
                        });
                        rects.fields.push(("Scale".to_string(), r.response.rect));
                        ui.label("Decimal places");
                        let r = egui::ComboBox::from_id_salt("pack_decimals").selected_text(format!("{}", self.packs.pack.output.decimals)).show_ui(ui, |ui| {
                            for dp in 0..=theme::MAX_DECIMALS {
                                ui.selectable_value(&mut self.packs.pack.output.decimals, dp, format!("{}", dp));
                            }
                        });
                        rects.fields.push(("Decimals".to_string(), r.response.rect));
                    });
                });
                // ---- the run
                theme::card(ui, "Run the pack", |ui| {
                    for i in 0..self.packs.pack.inputs.len() {
                        let slot = self.packs.pack.inputs[i].slot.clone();
                        ui.horizontal(|ui| {
                            ui.label(format!("{}:", slot));
                            let f = self.packs.files.entry(slot.clone()).or_default();
                            let r = ui.add(egui::TextEdit::singleline(f).desired_width(theme::FIELD_W_SORT_LIST * 2.0).hint_text("this month's file"));
                            rects.fields.push((format!("File of {}", slot), r.rect));
                            let b = ui.button("Choose\u{2026}");
                            rects.buttons.push((format!("Choose {}", slot), b.rect));
                            if b.clicked() {
                                act = Some(Act::Choose(slot));
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Cutoff date (YYYY-MM-DD):");
                        let r = ui.add(egui::TextEdit::singleline(&mut self.packs.cutoff).desired_width(theme::CF_DEFAULT_W));
                        rects.fields.push(("Cutoff".to_string(), r.rect));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Workbook:");
                        let r = ui.add(egui::TextEdit::singleline(&mut self.packs.out_path).desired_width(theme::FIELD_W_SORT_LIST * 2.0).hint_text("next to the first file"));
                        rects.fields.push(("Workbook".to_string(), r.rect));
                        let b = ui.button("Choose\u{2026}");
                        rects.buttons.push(("Choose workbook".to_string(), b.rect));
                        if b.clicked() {
                            act = Some(Act::ChooseOut);
                        }
                    });
                });
                if let Some((is_err, text)) = &self.packs.message {
                    let color = if *is_err { tk.danger } else { tk.muted };
                    let r = ui.add(egui::Label::new(egui::RichText::new(text).color(color)).wrap());
                    rects.message = Some(r.rect);
                }
            });
            d.footer(|ui| {
                ui.horizontal_wrapped(|ui| {
                    let b = theme::primary_button(ui, "Run pack");
                    rects.buttons.push(("Run pack".to_string(), b.rect));
                    if b.clicked() {
                        act = Some(Act::Run);
                    }
                    let c = ui.button("Close");
                    rects.buttons.push(("Close".to_string(), c.rect));
                    if c.clicked() {
                        close = true;
                    }
                });
            });
        });
        match act {
            Some(Act::AddSlot) => self.pack_add_slot_from_tab(),
            Some(Act::AddItem) => self.pack_add_item_from_tab(),
            Some(Act::SlotTable(s)) => self.pack_add_slot_table(&s),
            Some(Act::RemoveSlot(i)) => self.pack_remove_slot(i),
            Some(Act::RemoveItem(i)) => {
                if i < self.packs.pack.items.len() {
                    self.packs.pack.items.remove(i);
                }
            }
            Some(Act::Up(i)) => {
                if i > 0 && i < self.packs.pack.items.len() {
                    self.packs.pack.items.swap(i, i - 1);
                }
            }
            Some(Act::Down(i)) => {
                if i + 1 < self.packs.pack.items.len() {
                    self.packs.pack.items.swap(i, i + 1);
                }
            }
            Some(Act::Save) => self.pack_save(),
            Some(Act::Open(n)) => self.pack_open(&n),
            Some(Act::New) => {
                self.packs.pack = ReportPack::default();
                self.packs.opened_from.clear();
                self.packs.files.clear();
                self.packs.confirm_replace = false;
                self.packs.message = None;
            }
            Some(Act::Run) => self.pack_run(),
            Some(Act::Choose(slot)) => {
                #[cfg(not(test))]
                if let Some(p) = rfd::FileDialog::new().add_filter("Data Files (CSV, Parquet)", &["csv", "parquet"]).pick_file() {
                    self.packs.files.insert(slot, p.display().to_string());
                }
                #[cfg(test)]
                let _ = slot;
            }
            Some(Act::ChooseOut) => {
                #[cfg(not(test))]
                if let Some(p) = rfd::FileDialog::new().add_filter("Excel workbook", &["xlsx"]).set_file_name(format!("{}.xlsx", micro_parquet::paths::sanitize_file_name(&self.packs.pack.name))).save_file() {
                    self.packs.out_path = p.display().to_string();
                }
            }
            None => {}
        }
        self.packs.rects = rects;
        ModalOutcome { close, step: None }
    }
}
