//! The colour picker and View ▸ House colours… (Phase 8 W7).
//!
//! The picker opens from a swatch in the studio's "Colours" list, or from right-click ▸ Colour… on a mark. It offers Automatic, 24
//! swatches (the 8 house hues in Light, Base and Deep), a hex field and a tick box "Save as house colour". The warnings (hard to
//! tell from another item, low contrast, a status colour) are under it. The pick is the owner's call: a warning never refuses it.

use crate::ui::prelude::*;
use micro_parquet::charts::colours::{colour_names_of, house, pick_warnings, resolved, Colour, ColourMap, ColourPick, HexColour};
use micro_parquet::charts::tokens as tok;

/// The title of the picker.
pub const PICKER_TITLE: &str = "Colour";
/// The title of View ▸ House colours….
pub const HOUSE_TITLE: &str = "House colours";

/// Where the pick goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickTarget {
    /// The chart in the studio.
    #[default]
    Studio,
    /// A chart tab (its own snapshot of the chart).
    ChartTab(u64),
    /// The house colour of the name (no chart).
    House,
}

/// What the picker drew, for the render tests.
#[derive(Clone, Debug, Default)]
pub struct PickerRects {
    /// "Automatic", "Light Blue", "Base Blue", "Deep Blue" …, the hex box, "Use hex", the tick box, "Done".
    pub fields: Vec<(String, egui::Rect)>,
    /// The warning lines under the picker.
    pub warnings: Vec<egui::Rect>,
}

#[derive(Clone, Debug, Default)]
pub struct PickerState {
    pub target: PickTarget,
    /// The item being coloured.
    pub name: String,
    /// The text of the hex box, and why it is not a colour.
    pub hex: String,
    pub hex_error: Option<String>,
    /// "Save as house colour": the next pick is also written to the house colours file.
    pub save_house: bool,
    /// The swatch under the pointer in the last frame, named on the line below the grid.
    pub hovered: Option<String>,
    pub rects: PickerRects,
}

#[derive(Clone, Debug, Default)]
pub struct HouseState {
    /// The name typed in "Add".
    pub new_name: String,
    pub error: Option<String>,
    pub rects: PickerRects,
}

fn rgb32(rgb: u32) -> egui::Color32 {
    theme::hex(rgb)
}

impl MicroParquetApp {
    /// The colour map and the names of the chart a pick goes to, with what each name looks like now.
    fn picker_chart(&self, target: PickTarget) -> Option<(ColourMap, Vec<(String, Colour)>)> {
        let data = match target {
            PickTarget::Studio => self.chart.data.as_ref()?,
            PickTarget::ChartTab(id) => &self.chart_tabs.get(&id)?.data,
            PickTarget::House => return None,
        };
        let cm = data.spec.colour_map.clone();
        let names = colour_names_of(data);
        let all = resolved(&cm, &names);
        Some((cm, all))
    }

    pub(crate) fn open_colour_picker(&mut self, target: PickTarget, name: &str) {
        if let Err(e) = micro_parquet::charts::spec::ChartSpec::refuse_other(name) {
            self.status_message = format!("{:#}", e);
            return;
        }
        let hex = self.picker_chart(target).and_then(|(cm, _)| cm.get(name)).map(|c| tok::hex_text(c.hex(self.dark_mode))).unwrap_or_default();
        self.colour_picker = PickerState { target, name: name.to_string(), hex, hex_error: None, save_house: false, hovered: None, rects: PickerRects::default() };
        self.active_tool = ActiveToolModal::ColourPicker;
    }

    /// Right-click ▸ Colour…: the picker for the item a mark stands for (its series, or its part of a donut).
    pub(crate) fn open_colour_picker_of_mark(&mut self, target: PickTarget, m: &micro_parquet::charts::data::MarkRef) {
        let item = m.series.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| m.category.clone());
        let names: Vec<String> = match target {
            PickTarget::Studio => self.chart.data.as_ref().map(colour_names_of),
            PickTarget::ChartTab(id) => self.chart_tabs.get(&id).map(|i| colour_names_of(&i.data)),
            PickTarget::House => None,
        }
        .unwrap_or_default();
        if item != micro_parquet::chart::OTHER_LABEL && !names.contains(&item) {
            self.status_message = "This chart draws one colour for everything. Choose a Series to colour its items one by one.".to_string();
            return;
        }
        self.open_colour_picker(target, &item);
    }

    pub(crate) fn open_house_colours(&mut self) {
        self.house_dialog = HouseState::default();
        if let Some(e) = house::error() {
            self.house_dialog.error = Some(e);
        }
        self.active_tool = ActiveToolModal::HouseColours;
    }

    /// Puts `pick` (or Automatic, `None`) on the item `name` of the chart the picker was opened on.
    pub(crate) fn colour_apply(&mut self, target: PickTarget, name: &str, pick: Option<ColourPick>) {
        match target {
            PickTarget::Studio => {
                let mut apply = |m: &mut ColourMap| match pick {
                    Some(p) => m.set_pick(name, p),
                    None => m.clear_pick(name),
                };
                apply(&mut self.chart.spec.colour_map);
                if let Some(d) = self.chart.data.as_mut() {
                    apply(&mut d.spec.colour_map);
                }
                self.chart_save_choices();
            }
            PickTarget::ChartTab(id) => {
                let title = self.tabs.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_default();
                let Some(info) = self.chart_tabs.get_mut(&id) else { return };
                let mut apply = |m: &mut ColourMap| match pick {
                    Some(p) => m.set_pick(name, p),
                    None => m.clear_pick(name),
                };
                apply(&mut info.data.spec.colour_map);
                if let Some(n) = info.next_spec.as_mut() {
                    apply(&mut n.colour_map);
                }
                // kept with the session, with the chart's other settings (as Reset colours does)
                if let Ok(spec) = serde_json::to_value(&info.data.spec) {
                    let mut changed = false;
                    for e in self.settings.chart_tabs.iter_mut() {
                        if e.get("title").and_then(|t| t.as_str()) == Some(title.as_str()) {
                            e["spec"] = spec.clone();
                            changed = true;
                        }
                    }
                    if changed {
                        self.save_settings();
                    }
                }
            }
            PickTarget::House => {
                if let Some(p) = pick {
                    if let Err(e) = house::set(name, p) {
                        self.status_message = e;
                    }
                }
            }
        }
    }

    /// What the picker says about the item's pick now.
    fn picker_warnings(&self, target: PickTarget, name: &str) -> Vec<String> {
        let pick = match target {
            PickTarget::House => house::lookup(name),
            _ => self.picker_chart(target).and_then(|(cm, _)| cm.pick_of(name).copied().or_else(|| house::lookup(name))),
        };
        let Some(pick) = pick else { return Vec::new() };
        let others: Vec<(String, Colour)> = self.picker_chart(target).map(|(_, all)| all.into_iter().filter(|(n, _)| n != name).collect()).unwrap_or_default();
        pick_warnings(&pick, &others)
    }

    pub(crate) fn render_colour_picker_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let tk = *theme::tokens(self.dark_mode);
        let dark = self.dark_mode;
        let target = self.colour_picker.target;
        let name = self.colour_picker.name.clone();
        let mut rects = PickerRects::default();
        let mut chosen: Option<Option<ColourPick>> = None;
        let mut hovered_swatch: Option<String> = None;
        let hovered_prev = self.colour_picker.hovered.clone();
        let mut hex_text = self.colour_picker.hex.clone();
        let mut hex_error = self.colour_picker.hex_error.clone();
        let mut save_house = self.colour_picker.save_house;
        let own = self.picker_chart(target).and_then(|(cm, _)| cm.pick_of(&name).copied());
        let house_pick = house::lookup(&name);
        let shown = match target {
            PickTarget::House => house_pick,
            _ => own,
        };
        let title = match target {
            PickTarget::House => format!("{}  House colour of '{}'", theme::ph::PALETTE, name),
            _ => format!("{}  {} of '{}'", theme::ph::PALETTE, PICKER_TITLE, name),
        };
        let warnings = self.picker_warnings(target, &name);
        theme::modal(&title).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                if target != PickTarget::House {
                    let note = match (own, house_pick) {
                        (Some(_), _) => "This chart's own pick.".to_string(),
                        (None, Some(_)) => "The house colour of this name (View \u{25b8} House colours\u{2026}).".to_string(),
                        (None, None) => "The automatic colour of its place in the chart.".to_string(),
                    };
                    ui.label(egui::RichText::new(note).color(tk.muted));
                    let r = ui.add(egui::SelectableLabel::new(own.is_none(), "Automatic")).on_hover_text("The palette slot (or the house colour of this name), as before");
                    rects.fields.push(("Automatic".to_string(), r.rect));
                    if r.clicked() {
                        chosen = Some(None);
                    }
                    ui.add_space(theme::SPACE_XS);
                }
                // the 24 swatches: Light, Base and Deep of each of the 8 hues (Base is the palette slot and follows the theme)
                egui::Grid::new("colour_swatches").num_columns(9).spacing(egui::vec2(theme::SPACE_XS, theme::SPACE_XS)).show(ui, |ui| {
                    for (shade, label) in ["Light", "Base", "Deep"].iter().enumerate() {
                        ui.label(egui::RichText::new(*label).size(theme::TEXT_LABEL).color(tk.muted));
                        for hue in 0..tok::SLOTS {
                            let pick = ColourPick::swatch(hue, shade);
                            let fill = rgb32(pick.colour().hex(dark));
                            let on = shown == Some(pick);
                            let (rect, r) = ui.allocate_exact_size(egui::vec2(theme::SWATCH, theme::SWATCH), egui::Sense::click());
                            ui.painter().rect_filled(rect, egui::Rounding::same(theme::RADIUS_CONTROL), fill);
                            if on {
                                ui.painter().rect_stroke(rect.expand(2.0), egui::Rounding::same(theme::RADIUS_CONTROL), egui::Stroke::new(2.0, tk.ink));
                            }
                            let label = format!("{} {}", label, tok::HUE_NAMES[hue]);
                            let hexs = tok::hex_text(pick.colour().hex(dark));
                            if r.hovered() {
                                hovered_swatch = Some(format!("{} {}{}", label, hexs, if shade == 1 { " (the palette slot: it follows the theme)" } else { "" }));
                            }
                            rects.fields.push((label, r.rect));
                            if r.clicked() {
                                chosen = Some(Some(pick));
                            }
                        }
                        ui.end_row();
                    }
                });
                // what the swatch under the pointer is (a line, not a tooltip: a tooltip can sit under the pointer and swallow the click)
                let now = match shown {
                    Some(p) => format!("Now: {}", tok::hex_text(p.colour().hex(dark))),
                    None => "Now: automatic".to_string(),
                };
                let line = hovered_prev.clone().unwrap_or(now);
                let r = ui.add(egui::Label::new(egui::RichText::new(line).size(theme::TEXT_LABEL).color(tk.muted)).wrap());
                rects.fields.push(("Swatch name".to_string(), r.rect));
                ui.add_space(theme::SPACE_S);
                // any hex
                ui.horizontal(|ui| {
                    ui.label("Hex");
                    let r = ui.add(egui::TextEdit::singleline(&mut hex_text).hint_text("#rrggbb").desired_width(theme::SWATCH_HEX_W));
                    rects.fields.push(("Hex".to_string(), r.rect));
                    let apply = ui.button("Use hex");
                    rects.fields.push(("Use hex".to_string(), apply.rect));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if apply.clicked() || enter {
                        match HexColour::parse(&hex_text) {
                            Ok(h) => {
                                hex_error = None;
                                chosen = Some(Some(ColourPick::Hex { hex: h }));
                            }
                            Err(e) => hex_error = Some(e),
                        }
                    }
                });
                if let Some(e) = &hex_error {
                    let r = ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap());
                    rects.fields.push(("Hex error".to_string(), r.rect));
                }
                if target != PickTarget::House {
                    let r = ui.checkbox(&mut save_house, "Save as house colour").on_hover_text("Every chart, template, pack chart and chart book colours this name the same way, unless the chart has its own pick");
                    rects.fields.push(("Save as house colour".to_string(), r.rect));
                }
                // the warnings: the pick is yours, this is only what it costs
                for w in &warnings {
                    let before = ui.cursor().min;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(theme::ph::WARNING).color(tk.warning));
                        ui.add(egui::Label::new(egui::RichText::new(w).color(tk.ink)).wrap());
                    });
                    rects.warnings.push(egui::Rect::from_min_max(before, ui.cursor().min));
                }
            });
            d.footer(|ui| {
                let r = theme::primary_button(ui, "Done");
                rects.fields.push(("Done".to_string(), r.rect));
                if r.clicked() {
                    close = true;
                }
            });
        });
        self.colour_picker.hovered = hovered_swatch;
        self.colour_picker.hex = hex_text;
        self.colour_picker.hex_error = hex_error;
        self.colour_picker.save_house = save_house;
        self.colour_picker.rects = rects;
        if let Some(pick) = chosen {
            if let Some(p) = pick {
                self.colour_picker.hex = tok::hex_text(p.colour().hex(dark));
            }
            self.colour_apply(target, &name, pick);
            if save_house && target != PickTarget::House {
                if let Some(p) = pick {
                    match house::set(&name, p) {
                        Ok(()) => self.status_message = format!("'{}' is now {} in every chart (House colours).", name, tok::hex_text(p.colour().hex(dark))),
                        Err(e) => self.status_message = e,
                    }
                }
            }
        }
        if close && target == PickTarget::House {
            // back to the list the colour was picked from
            self.active_tool = ActiveToolModal::HouseColours;
            return ModalOutcome { close: false, step: None };
        }
        ModalOutcome { close, step: None }
    }

    pub(crate) fn render_house_colours_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let mut close = false;
        let tk = *theme::tokens(self.dark_mode);
        let dark = self.dark_mode;
        let mut rects = PickerRects::default();
        let list = house::list();
        let mut remove: Option<String> = None;
        let mut edit: Option<String> = None;
        let mut add: Option<String> = None;
        let mut new_name = self.house_dialog.new_name.clone();
        let broken = house::error();
        theme::modal(&format!("{}  {}", theme::ph::PALETTE, HOUSE_TITLE)).default_width(theme::DIALOG_W_TOOL).show(ctx, |d| {
            d.body(|ui| {
                ui.add(egui::Label::new("A colour for a name, in every chart, template, pack chart and chart book, unless the chart has its own pick for it. Names are matched without spaces at the ends and in any case. Reset colours in a chart leaves them as they are.").wrap());
                ui.add_space(theme::SPACE_S);
                if let Some(e) = &broken {
                    let r = ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap());
                    rects.fields.push(("File error".to_string(), r.rect));
                }
                if list.is_empty() && broken.is_none() {
                    let r = ui.label(egui::RichText::new("No house colours yet. Add a name below, or tick \"Save as house colour\" in the colour picker of a chart.").color(tk.muted));
                    rects.fields.push(("Empty".to_string(), r.rect));
                }
                egui::Grid::new("house_colour_rows").num_columns(4).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                    for h in &list {
                        let (rect, r) = ui.allocate_exact_size(egui::vec2(theme::SWATCH, theme::SWATCH), egui::Sense::click());
                        ui.painter().rect_filled(rect, egui::Rounding::same(theme::RADIUS_CONTROL), rgb32(h.pick.colour().hex(dark)));
                        let r = r.on_hover_text("Change the colour");
                        rects.fields.push((format!("Swatch {}", h.name), r.rect));
                        if r.clicked() {
                            edit = Some(h.name.clone());
                        }
                        let r = ui.label(&h.name);
                        rects.fields.push((format!("Name {}", h.name), r.rect));
                        ui.label(egui::RichText::new(tok::hex_text(h.pick.colour().hex(dark))).color(tk.muted));
                        let r = ui.button("Remove");
                        rects.fields.push((format!("Remove {}", h.name), r.rect));
                        if r.clicked() {
                            remove = Some(h.name.clone());
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(theme::SPACE_S);
                ui.horizontal(|ui| {
                    ui.label("Add");
                    let r = ui.add(egui::TextEdit::singleline(&mut new_name).hint_text("name, for example TD").desired_width(theme::SWATCH_NAME_W));
                    rects.fields.push(("New name".to_string(), r.rect));
                    let b = ui.button("Add");
                    rects.fields.push(("Add".to_string(), b.rect));
                    if b.clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        add = Some(new_name.clone());
                    }
                });
                if let Some(e) = &self.house_dialog.error {
                    let r = ui.add(egui::Label::new(egui::RichText::new(e).color(tk.danger)).wrap());
                    rects.fields.push(("Error".to_string(), r.rect));
                }
            });
            d.footer(|ui| {
                let r = theme::primary_button(ui, "Done");
                rects.fields.push(("Done".to_string(), r.rect));
                if r.clicked() {
                    close = true;
                }
            });
        });
        self.house_dialog.new_name = new_name;
        self.house_dialog.rects = rects;
        if let Some(n) = remove {
            match house::remove(&n) {
                Ok(()) => self.house_dialog.error = None,
                Err(e) => self.house_dialog.error = Some(e),
            }
        }
        if let Some(n) = add {
            let n = n.trim().to_string();
            if n.is_empty() {
                self.house_dialog.error = Some("Type the name first, for example TD.".to_string());
            } else if micro_parquet::charts::spec::ChartSpec::refuse_other(&n).is_err() {
                self.house_dialog.error = Some("\"Other\" is Top N's remainder and always has its own grey.".to_string());
            } else {
                // a new house colour starts as the first palette colour; the picker changes it
                match house::set(&n, ColourPick::swatch(0, 1)) {
                    Ok(()) => {
                        self.house_dialog.error = None;
                        self.house_dialog.new_name.clear();
                        edit = Some(n);
                    }
                    Err(e) => self.house_dialog.error = Some(e),
                }
            }
        }
        if let Some(n) = edit {
            self.open_colour_picker(PickTarget::House, &n);
            return ModalOutcome { close: false, step: None };
        }
        ModalOutcome { close, step: None }
    }
}
