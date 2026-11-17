//! The single top bar (44 px): app mark, File / View / Help, the toolbar groups, then the command
//! search box at the far right. Groups that do not fit fold into a "More" menu, so nothing
//! overlaps at any window width.

use crate::ui::prelude::*;
use micro_parquet::commands::{self as cmds, CommandGroup, CommandId};

/// One item of the toolbar: a command button, or one of the two menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKey {
    Cmd(CommandId),
    Banking,
    Export,
}

/// The toolbar items in the order they are drawn, with the group each belongs to (groups are
/// separated by a thin line): Open / Stack / Batch | Filter / Sort / Columns / Calc column /
/// Pivot | Banking | Export.
pub const BAR_ITEMS: [(BarKey, usize); 10] = [
    (BarKey::Cmd(CommandId::OpenFile), 0),
    (BarKey::Cmd(CommandId::Stack), 0),
    (BarKey::Cmd(CommandId::Batch), 0),
    (BarKey::Cmd(CommandId::Filter), 1),
    (BarKey::Cmd(CommandId::Sort), 1),
    (BarKey::Cmd(CommandId::Columns), 1),
    (BarKey::Cmd(CommandId::CalcColumn), 1),
    (BarKey::Cmd(CommandId::Pivot), 1),
    (BarKey::Banking, 2),
    (BarKey::Export, 3),
];

/// The order in which items fold into More when space runs out, first to last. Open, Banking and
/// Export are not in it: they never fold.
pub const FOLD_ORDER: [BarKey; 7] = [
    BarKey::Cmd(CommandId::CalcColumn),
    BarKey::Cmd(CommandId::Columns),
    BarKey::Cmd(CommandId::Batch),
    BarKey::Cmd(CommandId::Stack),
    BarKey::Cmd(CommandId::Sort),
    BarKey::Cmd(CommandId::Filter),
    BarKey::Cmd(CommandId::Pivot),
];

/// The states the bar goes through as the window gets narrower: (items folded, icon-only).
/// The first item folds, and if space is still short the buttons become icon-only (with
/// tooltips) before another item folds; from then on items fold one by one in icon-only mode.
pub const LADDER: [(usize, bool); 8] =
    [(0, false), (1, false), (1, true), (2, true), (3, true), (4, true), (5, true), (6, true)];

/// How the bar is laid out: how many items of `FOLD_ORDER` are in More, whether the buttons show
/// only their icons, and how wide the search box is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPlan {
    pub folded: usize,
    pub icon_only: bool,
    pub search_w: f32,
}

impl BarPlan {
    pub fn is_folded(&self, key: BarKey) -> bool {
        FOLD_ORDER.iter().take(self.folded).any(|k| *k == key)
    }
}

/// Width of everything left of the search box in a state of the ladder.
fn left_width(fixed: f32, label_w: &[f32; 10], icon_w: &[f32; 10], more_w: f32, gap: f32, item_gap: f32, folded: usize, icon_only: bool) -> f32 {
    let plan = BarPlan { folded, icon_only, search_w: 0.0 };
    let mut w = fixed;
    let mut last_group = usize::MAX;
    let mut in_group = 0;
    for (i, (key, group)) in BAR_ITEMS.iter().enumerate() {
        if plan.is_folded(*key) {
            continue;
        }
        if *group != last_group {
            w += gap;
            last_group = *group;
            in_group = 0;
        }
        if in_group > 0 {
            w += item_gap;
        }
        in_group += 1;
        w += if icon_only { icon_w[i] } else { label_w[i] };
    }
    w + gap + more_w
}

/// Decides the state of the bar: the first state of the ladder in which everything fits with the
/// search box at least `SEARCH_MIN_W` wide. `avail` is the width of the bar's content, `fixed`
/// the mark and the File / View / Help menus, `label_w` / `icon_w` the width of each item of
/// `BAR_ITEMS` with its label and icon-only, `more_w` the More button, `gap` what lies between
/// two groups and `item_gap` between two items. When even the last state does not fit, it is
/// still the last state: Open, Banking and Export are never folded.
pub fn plan_bar(avail: f32, fixed: f32, label_w: &[f32; 10], icon_w: &[f32; 10], more_w: f32, gap: f32, item_gap: f32) -> BarPlan {
    let mut chosen = LADDER[LADDER.len() - 1];
    for (folded, icon_only) in LADDER {
        let left = left_width(fixed, label_w, icon_w, more_w, gap, item_gap, folded, icon_only);
        if left + gap + theme::SEARCH_MIN_W <= avail {
            chosen = (folded, icon_only);
            break;
        }
    }
    let left = left_width(fixed, label_w, icon_w, more_w, gap, item_gap, chosen.0, chosen.1);
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

/// The text of an item's button, with its label or icon-only.
fn item_text(key: BarKey, icon_only: bool) -> String {
    match key {
        BarKey::Cmd(id) => {
            let label = crate::ui::commands::toolbar_label(id);
            let icon = crate::ui::commands::command_icon(id);
            if icon_only || label.is_empty() {
                icon.to_string()
            } else {
                format!("{}  {}", icon, label)
            }
        }
        BarKey::Banking => {
            if icon_only {
                menu_icon_label(theme::ph::BANK)
            } else {
                menu_label(theme::ph::BANK, "Banking")
            }
        }
        BarKey::Export => {
            if icon_only {
                menu_icon_label(theme::ph::EXPORT)
            } else {
                menu_label(theme::ph::EXPORT, "Export")
            }
        }
    }
}

/// The name an icon-only item shows as its tooltip.
fn item_name(key: BarKey) -> &'static str {
    match key {
        BarKey::Cmd(id) => crate::ui::commands::toolbar_label(id),
        BarKey::Banking => "Banking",
        BarKey::Export => "Export",
    }
}

/// The widths the plan needs, measured with the fonts and spacing the bar is drawn with.
pub struct BarWidths {
    pub fixed: f32,
    pub label: [f32; 10],
    pub icon: [f32; 10],
    pub more: f32,
    pub gap: f32,
    pub item_gap: f32,
}

fn measure(ui: &egui::Ui) -> BarWidths {
    let sp = ui.spacing().item_spacing.x;
    let fixed = theme::APP_MARK_SIZE + ["File", "View", "Help"].iter().map(|t| sp + button_w(ui, t)).sum::<f32>();
    let mut label = [0.0; 10];
    let mut icon = [0.0; 10];
    for (i, (key, _)) in BAR_ITEMS.iter().enumerate() {
        label[i] = button_w(ui, &item_text(*key, false));
        icon[i] = button_w(ui, &item_text(*key, true));
    }
    let more = button_w(ui, &menu_label(theme::ph::DOTS_THREE, "More"));
    BarWidths { fixed, label, icon, more, gap: sp + theme::BAR_SEPARATOR_W + sp, item_gap: sp }
}

/// The plan for a bar `avail` wide, measured with `ui`'s fonts and spacing.
pub fn plan_for(ui: &egui::Ui, avail: f32) -> BarPlan {
    let w = measure(ui);
    plan_bar(avail, w.fixed, &w.label, &w.icon, w.more, w.gap, w.item_gap)
}

/// A drawn item of the bar and where it is (tests check that none overlap).
pub type BarRects = Vec<(String, egui::Rect)>;

impl MicroParquetApp {
    /// Draws the bar. Returns the rectangle of every item drawn.
    pub(crate) fn render_top_bar(&mut self, ctx: &egui::Context) -> BarRects {
        let tk = *theme::tokens(self.dark_mode);
        let mut rects: BarRects = Vec::new();
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
                let plan = plan_for(ui, full.width());
                let gap = ui.spacing().item_spacing.x;
                let search_rect = egui::Rect::from_min_max(egui::pos2(full.right() - plan.search_w, full.top()), full.max);
                let left_rect = egui::Rect::from_min_max(full.min, egui::pos2(search_rect.left() - gap, full.bottom()));
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(left_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| self.draw_bar_left(ui, ctx, &plan, &mut rects),
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

    fn draw_bar_left(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, plan: &BarPlan, rects: &mut BarRects) {
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

        let r = self.file_menu(ui, ctx);
        rects.push(("File".to_string(), r.rect));
        let r = self.view_menu(ui, ctx);
        rects.push(("View".to_string(), r.rect));
        let r = ui
            .menu_button("Help", |ui| {
                if ui.button("About MicroParquet").clicked() {
                    ui.close_menu();
                    self.show_about = true;
                }
            })
            .response;
        rects.push(("Help".to_string(), r.rect));

        // the items that are not folded, group by group, each group after a thin line
        let mut last_group = usize::MAX;
        for (key, group) in BAR_ITEMS.iter() {
            if plan.is_folded(*key) {
                continue;
            }
            if *group != last_group {
                bar_separator(ui, tk.border);
                last_group = *group;
            }
            let (name, rect) = match key {
                BarKey::Cmd(id) => {
                    let r = if *group == 1 {
                        ui.add_enabled_ui(has_tab, |ui| self.toolbar_button(ui, ctx, *id, plan.icon_only)).inner
                    } else {
                        self.toolbar_button(ui, ctx, *id, plan.icon_only)
                    };
                    (format!("{:?}", id), r.rect)
                }
                BarKey::Banking | BarKey::Export => {
                    let group_id = if *key == BarKey::Banking { CommandGroup::Banking } else { CommandGroup::Export };
                    let text = item_text(*key, plan.icon_only);
                    let r = ui
                        .add_enabled_ui(has_tab, |ui| {
                            ui.menu_button(text, |ui| {
                                let mut last_section: Option<&str> = None;
                                for c in cmds::in_group(group_id) {
                                    // the Banking menu has two headings: Treasury & ALM and Loans & credit
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
                    let r = if plan.icon_only { r.on_hover_text(item_name(*key)) } else { r };
                    (item_name(*key).to_string(), r.rect)
                }
            };
            rects.push((name, rect));
        }

        // More: the items that were folded (in toolbar order), then the commands that have no
        // toolbar button.
        bar_separator(ui, tk.border);
        let folded: Vec<BarKey> = BAR_ITEMS.iter().map(|(k, _)| *k).filter(|k| plan.is_folded(*k)).collect();
        let r = ui
            .add_enabled_ui(has_tab || !folded.is_empty(), |ui| {
                ui.menu_button(menu_label(theme::ph::DOTS_THREE, "More"), |ui| {
                    self.more_menu(ui, ctx, &folded);
                })
                .response
            })
            .inner;
        rects.push(("More".to_string(), r.rect));
    }

    /// The More menu: the folded items first, then the commands without a toolbar button.
    fn more_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, folded: &[BarKey]) {
        for key in folded {
            if let BarKey::Cmd(id) = key {
                self.command_menu_item(ui, ctx, *id);
            }
        }
        if !folded.is_empty() {
            ui.separator();
        }
        for id in cmds::TOOLBAR_MORE {
            self.command_menu_item(ui, ctx, *id);
        }
    }
}

/// A thin vertical line between two groups.
fn bar_separator(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(theme::BAR_SEPARATOR_W, theme::BAR_SEPARATOR_H), egui::Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), theme::line(color));
}
