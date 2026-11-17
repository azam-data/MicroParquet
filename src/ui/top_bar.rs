//! The single top bar (44 px): app mark, File / View / Help, the toolbar groups, then the command
//! search box at the far right. Groups that do not fit fold into a "More" menu, so nothing
//! overlaps at any window width.

use crate::ui::prelude::*;
use micro_parquet::commands::{self as cmds, CommandGroup, CommandId};

/// A toolbar group, in the order it is drawn (and the reverse of the order it folds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarGroup {
    /// Open / Stack / Batch.
    Files,
    /// Filter / Sort / Columns / Calc column / Pivot.
    Transform,
    /// The Banking menu.
    Banking,
    /// The Export menu.
    Export,
}

pub const BAR_GROUPS: [BarGroup; 4] = [BarGroup::Files, BarGroup::Transform, BarGroup::Banking, BarGroup::Export];

/// Which groups are drawn and how wide the search box is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPlan {
    /// The first `shown` groups are drawn; the rest are in the More menu.
    pub shown: usize,
    pub search_w: f32,
}

/// Decides how many groups fit. `avail` is the width of the bar's content, `fixed` the mark and
/// the File / View / Help menus, `group_w` each group's width, `more_w` the More button and
/// `gap` what lies between two items (spacing and separator). The search box gets what is left,
/// at least `SEARCH_MIN_W` while there is room for it.
pub fn plan_bar(avail: f32, fixed: f32, group_w: &[f32], more_w: f32, gap: f32) -> BarPlan {
    let left = |k: usize| -> f32 {
        fixed + group_w.iter().take(k).map(|w| gap + w).sum::<f32>() + gap + more_w
    };
    let mut shown = 0;
    for k in 1..=group_w.len() {
        if left(k) + gap + theme::SEARCH_MIN_W <= avail {
            shown = k;
        } else {
            break;
        }
    }
    let room = avail - left(shown) - gap;
    BarPlan { shown, search_w: room.clamp(theme::SEARCH_ICON_W, theme::SEARCH_MAX_W) }
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

fn commands_width(ui: &egui::Ui, ids: &[CommandId]) -> f32 {
    let texts: Vec<String> = ids
        .iter()
        .map(|id| {
            let label = crate::ui::commands::toolbar_label(*id);
            let icon = crate::ui::commands::command_icon(*id);
            if label.is_empty() {
                icon.to_string()
            } else {
                format!("{}  {}", icon, label)
            }
        })
        .collect();
    texts.iter().map(|t| button_w(ui, t)).sum::<f32>() + ui.spacing().item_spacing.x * ids.len().saturating_sub(1) as f32
}

/// The widths the plan needs, measured with the fonts and spacing the bar is drawn with.
pub struct BarWidths {
    pub fixed: f32,
    pub groups: [f32; 4],
    pub more: f32,
    pub gap: f32,
}

fn measure(ui: &egui::Ui) -> BarWidths {
    let sp = ui.spacing().item_spacing.x;
    let fixed = theme::APP_MARK_SIZE
        + ["File", "View", "Help"].iter().map(|t| sp + button_w(ui, t)).sum::<f32>();
    let groups = [
        commands_width(ui, cmds::TOOLBAR_BUTTONS_FILE),
        commands_width(ui, cmds::TOOLBAR_BUTTONS_TRANSFORM),
        button_w(ui, &menu_label(theme::ph::BANK, "Banking")),
        button_w(ui, &menu_label(theme::ph::EXPORT, "Export")),
    ];
    let more = button_w(ui, &menu_label(theme::ph::DOTS_THREE, "More"));
    BarWidths { fixed, groups, more, gap: sp + theme::BAR_SEPARATOR_W + sp }
}

/// The plan for a bar `avail` wide, measured with `ui`'s fonts and spacing.
pub fn plan_for(ui: &egui::Ui, avail: f32) -> BarPlan {
    let w = measure(ui);
    plan_bar(avail, w.fixed, &w.groups, w.more, w.gap)
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

        for group in BAR_GROUPS.iter().take(plan.shown) {
            bar_separator(ui, tk.border);
            match group {
                BarGroup::Files => {
                    for id in cmds::TOOLBAR_BUTTONS_FILE {
                        let r = self.toolbar_button(ui, ctx, *id);
                        rects.push((format!("{:?}", id), r.rect));
                    }
                }
                BarGroup::Transform => {
                    ui.add_enabled_ui(has_tab, |ui| {
                        for id in cmds::TOOLBAR_BUTTONS_TRANSFORM {
                            let r = self.toolbar_button(ui, ctx, *id);
                            rects.push((format!("{:?}", id), r.rect));
                        }
                    });
                }
                BarGroup::Banking => {
                    let r = ui
                        .add_enabled_ui(has_tab, |ui| {
                            ui.menu_button(menu_label(theme::ph::BANK, "Banking"), |ui| {
                                for c in cmds::in_group(CommandGroup::Banking) {
                                    self.command_menu_item(ui, ctx, c.id);
                                }
                            })
                            .response
                        })
                        .inner;
                    rects.push(("Banking".to_string(), r.rect));
                }
                BarGroup::Export => {
                    let r = ui
                        .add_enabled_ui(has_tab, |ui| {
                            ui.menu_button(menu_label(theme::ph::EXPORT, "Export"), |ui| {
                                for c in cmds::in_group(CommandGroup::Export) {
                                    self.command_menu_item(ui, ctx, c.id);
                                }
                            })
                            .response
                        })
                        .inner;
                    rects.push(("Export".to_string(), r.rect));
                }
            }
        }

        // More: the groups that did not fit, then the commands that have no button.
        bar_separator(ui, tk.border);
        let folded: Vec<BarGroup> = BAR_GROUPS.iter().skip(plan.shown).copied().collect();
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

    /// The More menu: folded groups first (buttons as items, menus as sub-menus), then the
    /// commands without a toolbar button.
    fn more_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, folded: &[BarGroup]) {
        for group in folded {
            match group {
                BarGroup::Files => {
                    for id in cmds::TOOLBAR_BUTTONS_FILE {
                        self.command_menu_item(ui, ctx, *id);
                    }
                }
                BarGroup::Transform => {
                    for id in cmds::TOOLBAR_BUTTONS_TRANSFORM {
                        self.command_menu_item(ui, ctx, *id);
                    }
                }
                BarGroup::Banking => {
                    ui.menu_button(format!("{}  Banking", theme::ph::BANK), |ui| {
                        for c in cmds::in_group(CommandGroup::Banking) {
                            self.command_menu_item(ui, ctx, c.id);
                        }
                    });
                }
                BarGroup::Export => {
                    ui.menu_button(format!("{}  Export", theme::ph::EXPORT), |ui| {
                        for c in cmds::in_group(CommandGroup::Export) {
                            self.command_menu_item(ui, ctx, c.id);
                        }
                    });
                }
            }
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
