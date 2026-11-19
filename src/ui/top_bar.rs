//! The single top bar (44 px): app mark, the menus File / Edit / View / Banking / Export / Help, the
//! toolbar buttons, then the command search box at the far right. The menus never fold; buttons that
//! do not fit fold (from the right) into a "»" menu, so nothing overlaps at any window width.

use crate::ui::prelude::*;
use micro_parquet::commands::{self as cmds, CommandGroup, CommandId};

/// The toolbar buttons in the order they are drawn, with the group each belongs to (groups are
/// separated by a thin line): Open / Stack / Batch | Filter / Sort / Columns / Calc column / Pivot /
/// Rate profile | Chart / SQL. View ▸ "Toolbar buttons" switches each one off.
pub const BAR_ITEMS: [(CommandId, usize); 11] = [
    (CommandId::OpenFile, 0),
    (CommandId::Stack, 0),
    (CommandId::Batch, 0),
    (CommandId::Filter, 1),
    (CommandId::Sort, 1),
    (CommandId::Columns, 1),
    (CommandId::CalcColumn, 1),
    (CommandId::Pivot, 1),
    (CommandId::RateProfile, 1),
    (CommandId::ToggleChart, 2),
    (CommandId::ToggleSql, 2),
];

/// The menus, left to right. They never fold: only the buttons after them do.
pub const MENUS: [&str; 6] = ["File", "Edit", "View", "Banking", "Export", "Help"];

/// How the bar is laid out: how many of the visible buttons (counted from the right) are in the
/// overflow menu "»", whether the buttons show only their icons, and how wide the search box is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPlan {
    pub folded: usize,
    pub icon_only: bool,
    pub search_w: f32,
}

impl BarPlan {
    /// True when the button at `index` of `n` visible ones is in the overflow menu.
    pub fn is_folded(&self, index: usize, n: usize) -> bool {
        index >= n.saturating_sub(self.folded)
    }
}

/// The states the bar goes through as the window gets narrower for `n` visible buttons: (folded,
/// icon-only). Labels first, then icons only, then the buttons fold one by one from the right
/// (still icon-only) into "»".
pub fn ladder(n: usize) -> Vec<(usize, bool)> {
    let mut v = vec![(0, false), (0, true)];
    v.extend((1..=n).map(|f| (f, true)));
    v
}

/// Width of everything left of the search box in a state of the ladder.
fn left_width(fixed: f32, label_w: &[f32], icon_w: &[f32], groups: &[usize], overflow_w: f32, gap: f32, item_gap: f32, folded: usize, icon_only: bool) -> f32 {
    let n = label_w.len();
    let plan = BarPlan { folded, icon_only, search_w: 0.0 };
    let mut w = fixed;
    let mut last_group = usize::MAX;
    let mut in_group = 0;
    for i in 0..n {
        if plan.is_folded(i, n) {
            continue;
        }
        if groups[i] != last_group {
            w += gap;
            last_group = groups[i];
            in_group = 0;
        }
        if in_group > 0 {
            w += item_gap;
        }
        in_group += 1;
        w += if icon_only { icon_w[i] } else { label_w[i] };
    }
    if folded > 0 {
        w += gap + overflow_w;
    }
    w
}

/// Decides the state of the bar: the first state of the ladder in which everything fits with the
/// search box at least `SEARCH_MIN_W` wide. `avail` is the width of the bar's content, `fixed` the
/// mark and the six menus, `label_w` / `icon_w` / `groups` describe the visible buttons, `overflow_w`
/// is the "»" button, `gap` what lies between two groups and `item_gap` between two items. When even
/// the last state does not fit, it is still the last state: the menus are never folded.
pub fn plan_bar(avail: f32, fixed: f32, label_w: &[f32], icon_w: &[f32], groups: &[usize], overflow_w: f32, gap: f32, item_gap: f32) -> BarPlan {
    let states = ladder(label_w.len());
    let mut chosen = states[states.len() - 1];
    for (folded, icon_only) in &states {
        let left = left_width(fixed, label_w, icon_w, groups, overflow_w, gap, item_gap, *folded, *icon_only);
        if left + gap + theme::SEARCH_MIN_W <= avail {
            chosen = (*folded, *icon_only);
            break;
        }
    }
    let left = left_width(fixed, label_w, icon_w, groups, overflow_w, gap, item_gap, chosen.0, chosen.1);
    let room = avail - left - gap;
    BarPlan { folded: chosen.0, icon_only: chosen.1, search_w: room.clamp(theme::SEARCH_ICON_W, theme::SEARCH_MAX_W) }
}

/// Width of a text in the body font.
fn text_w(ui: &egui::Ui, text: &str) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), egui::FontId::new(theme::TEXT_BASE, egui::FontFamily::Proportional), egui::Color32::WHITE)
            .size()
            .x
    })
}

/// Width of a button with this text.
fn button_w(ui: &egui::Ui, text: &str) -> f32 {
    text_w(ui, text) + 2.0 * ui.spacing().button_padding.x + theme::BAR_WIDTH_SLACK
}

fn menu_label(icon: &str, label: &str) -> String {
    format!("{}  {}  {}", icon, label, theme::ph::CARET_DOWN)
}

fn menu_icon_label(icon: &str) -> String {
    format!("{}  {}", icon, theme::ph::CARET_DOWN)
}

/// The text of a button, with its label or icon-only.
fn item_text(id: CommandId, icon_only: bool) -> String {
    let label = crate::ui::commands::toolbar_label(id);
    let icon = crate::ui::commands::command_icon(id);
    if icon_only || label.is_empty() {
        icon.to_string()
    } else {
        format!("{}  {}", icon, label)
    }
}

/// The text of a menu button of the bar.
fn menu_text(name: &str) -> String {
    match name {
        "Banking" => menu_label(theme::ph::BANK, "Banking"),
        "Export" => menu_label(theme::ph::EXPORT, "Export"),
        other => other.to_string(),
    }
}

/// The widths the plan needs, measured with the fonts and spacing the bar is drawn with.
pub struct BarWidths {
    pub fixed: f32,
    pub label: Vec<f32>,
    pub icon: Vec<f32>,
    pub groups: Vec<usize>,
    pub overflow: f32,
    pub gap: f32,
    pub item_gap: f32,
}

fn measure(ui: &egui::Ui, visible: &[(CommandId, usize)]) -> BarWidths {
    let sp = ui.spacing().item_spacing.x;
    let fixed = theme::APP_MARK_SIZE + MENUS.iter().map(|t| sp + button_w(ui, &menu_text(t))).sum::<f32>();
    BarWidths {
        fixed,
        label: visible.iter().map(|(id, _)| button_w(ui, &item_text(*id, false))).collect(),
        icon: visible.iter().map(|(id, _)| button_w(ui, &item_text(*id, true))).collect(),
        groups: visible.iter().map(|(_, g)| *g).collect(),
        overflow: button_w(ui, OVERFLOW_LABEL),
        gap: sp + theme::BAR_SEPARATOR_W + sp,
        item_gap: sp,
    }
}

/// The text of the overflow button.
pub const OVERFLOW_LABEL: &str = "\u{bb}";

/// The plan for a bar `avail` wide with these visible buttons, measured with `ui`'s fonts and spacing.
pub fn plan_for(ui: &egui::Ui, avail: f32, visible: &[(CommandId, usize)]) -> BarPlan {
    let w = measure(ui, visible);
    plan_bar(avail, w.fixed, &w.label, &w.icon, &w.groups, w.overflow, w.gap, w.item_gap)
}

/// A drawn item of the bar and where it is (tests check that none overlap).
pub type BarRects = Vec<(String, egui::Rect)>;

impl MicroParquetApp {
    /// The toolbar buttons that are switched on in View ▸ Toolbar buttons.
    pub(crate) fn visible_bar_items(&self) -> Vec<(CommandId, usize)> {
        BAR_ITEMS.iter().copied().filter(|(id, _)| !self.settings.toolbar_hidden.iter().any(|n| n == crate::ui::commands::toolbar_label(*id))).collect()
    }

    /// Draws the bar. Returns the rectangle of every item drawn.
    pub(crate) fn render_top_bar(&mut self, ctx: &egui::Context) -> BarRects {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects: BarRects = Vec::new();
        let visible = self.visible_bar_items();
        egui::TopBottomPanel::top("top_bar")
            .exact_height(theme::TOP_BAR_H)
            .frame(egui::Frame::none().fill(tk.surface).inner_margin(egui::Margin::symmetric(theme::SPACE_M, 0.0)))
            .show(ctx, |ui| {
                theme::bottom_border(ui, tk.border);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.spacing_mut().item_spacing.x = theme::BAR_ITEM_GAP;
                ui.spacing_mut().button_padding.x = theme::BAR_BUTTON_PAD_X;
                let full = ui.max_rect();
                let plan = plan_for(ui, full.width(), &visible);
                let gap = ui.spacing().item_spacing.x;
                let search_rect = egui::Rect::from_min_max(egui::pos2(full.right() - plan.search_w, full.top()), full.max);
                let left_rect = egui::Rect::from_min_max(full.min, egui::pos2(search_rect.left() - gap, full.bottom()));
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(left_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| self.draw_bar_left(ui, ctx, &plan, &visible, &mut rects),
                );
                let r = ui
                    .allocate_new_ui(
                        egui::UiBuilder::new().max_rect(search_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| self.command_box(ui, plan.search_w),
                    )
                    .inner;
                rects.push(("Search".to_string(), r.rect));
            });
        rects
    }

    fn draw_bar_left(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, plan: &BarPlan, visible: &[(CommandId, usize)], rects: &mut BarRects) {
        let tk = *theme::tokens(self.dark_mode);
        let has_tab = !self.tabs.is_empty();

        // app mark
        let (rect, _) = ui.allocate_exact_size(egui::vec2(theme::APP_MARK_SIZE, theme::APP_MARK_SIZE), egui::Sense::hover());
        ui.painter().rect_filled(rect, theme::APP_MARK_RADIUS, tk.accent);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "M",
            egui::FontId::new(theme::TEXT_MARK, theme::family_bold()),
            theme::on_accent(self.dark_mode),
        );
        rects.push(("Mark".to_string(), rect));

        // the menus: File, Edit, View, Banking, Export, Help (they never fold)
        let r = self.file_menu(ui, ctx);
        rects.push(("File".to_string(), r.rect));
        let r = self.edit_menu(ui, ctx);
        rects.push(("Edit".to_string(), r.rect));
        let r = self.view_menu(ui, ctx);
        rects.push(("View".to_string(), r.rect));
        for group_id in [CommandGroup::Banking, CommandGroup::Export] {
            let name = if group_id == CommandGroup::Banking { "Banking" } else { "Export" };
            let text = menu_text(name);
            let r = ui
                .add_enabled_ui(has_tab, |ui| {
                    ui.menu_button(text, |ui| {
                        // compact rows: the Banking menu has about twenty entries and four headings, and must
                        // fit a 686 pt window under the top bar
                        ui.spacing_mut().interact_size.y = theme::MENU_ROW_H;
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let mut last_section: Option<&str> = None;
                        for c in cmds::in_group(group_id) {
                            let section = if group_id == CommandGroup::Banking { cmds::banking_section(c.id) } else { None };
                            if section.is_some() && section != last_section {
                                if last_section.is_some() {
                                    ui.separator();
                                }
                                ui.label(theme::label_text(section.unwrap_or_default()).color(tk.muted));
                                last_section = section;
                            }
                            self.command_menu_item(ui, ctx, c.id);
                        }
                    })
                    .response
                })
                .inner;
            rects.push((name.to_string(), r.rect));
        }
        let r = ui
            .menu_button("Help", |ui| {
                if ui.button("About MicroParquet").clicked() {
                    ui.close_menu();
                    self.show_about = true;
                }
            })
            .response;
        rects.push(("Help".to_string(), r.rect));

        // the buttons that are not folded, group by group, each group after a thin line
        let n = visible.len();
        let mut last_group = usize::MAX;
        for (i, (id, group)) in visible.iter().enumerate() {
            if plan.is_folded(i, n) {
                continue;
            }
            if *group != last_group {
                bar_separator(ui, tk.border);
                last_group = *group;
            }
            let r = if matches!(id, CommandId::OpenFile | CommandId::Stack | CommandId::Batch | CommandId::ToggleChart) {
                self.toolbar_button(ui, ctx, *id, plan.icon_only)
            } else {
                ui.add_enabled_ui(has_tab, |ui| self.toolbar_button(ui, ctx, *id, plan.icon_only)).inner
            };
            rects.push((format!("{:?}", id), r.rect));
        }

        // "»": the buttons that were folded, each as a menu item
        if plan.folded > 0 {
            bar_separator(ui, tk.border);
            let folded: Vec<CommandId> = visible.iter().enumerate().filter(|(i, _)| plan.is_folded(*i, n)).map(|(_, (id, _))| *id).collect();
            let r = ui
                .menu_button(OVERFLOW_LABEL, |ui| {
                    for id in &folded {
                        self.command_menu_item(ui, ctx, *id);
                    }
                })
                .response
                .on_hover_text("More toolbar buttons");
            rects.push(("Overflow".to_string(), r.rect));
        }
    }
}

/// A thin vertical line between two groups.
fn bar_separator(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(theme::BAR_SEPARATOR_W, theme::BAR_SEPARATOR_H), egui::Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), theme::line(color));
}
