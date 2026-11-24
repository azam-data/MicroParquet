//! A pivot in the Nested layout, drawn on its tab (Phase 9 P2): one "Row Labels" column in which each line is indented by its level,
//! parent lines and the Grand total in bold, then the pivot's value columns formatted as the flat table's are. A child module of
//! `table` so it draws numbers exactly as the flat grid does.

use super::*;
use micro_parquet::pivot_view::{NestedKind, NestedView};

/// Where a line of the Nested view was drawn (for the render tests: what is clicked, what is bold, how far it is indented).
#[derive(Clone, Debug)]
pub(crate) struct NestedLineReport {
    /// The line's index in the view.
    pub index: usize,
    pub row: egui::Rect,
    /// The label cell, and the text's own rectangle in it (its left edge is the indent).
    pub label_cell: egui::Rect,
    pub label_text: Option<egui::Rect>,
    /// The value cells' texts: the column's heading and the text's rectangle (right-aligned in its cell). A blank line has none.
    pub cells: Vec<(String, egui::Rect)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NestedGridReport {
    pub frame: Option<egui::Rect>,
    /// The heading cells, "Row Labels" first.
    pub header: Vec<(String, egui::Rect)>,
    pub lines: Vec<NestedLineReport>,
}

/// The report of the Nested grid drawn for `tab_id` in the last frame.
pub(crate) fn nested_grid_report(ctx: &egui::Context, tab_id: u64) -> Option<NestedGridReport> {
    ctx.data(|d| d.get_temp::<NestedGridReport>(egui::Id::new(("nested_grid_report", tab_id))))
}

/// Draws the Nested view. A double-click on a value cell (or "Show rows" in its menu) asks for the rows of that line and column; for the
/// Grand total and parent lines too.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_nested_grid(
    ui: &mut egui::Ui,
    view: &NestedView,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
    tab_id: u64,
    show_rows_off: Option<&str>,
) -> Option<GridAction> {
    use micro_parquet::table_layout::{column_min_width, MAX_COLUMN_WIDTH, SAMPLE_ROWS};
    let tk = *theme::tokens(ui.visuals().dark_mode);
    let ctx = ui.ctx().clone();
    let cols = view.values.get_columns();
    let n = view.rows.len();
    let mut action: Option<GridAction> = None;
    let mut report = NestedGridReport::default();
    let want_y = ui.ctx().data_mut(|d| d.remove_temp::<f32>(egui::Id::new(("grid_scroll_y", tab_id))));

    // widths: the heading, the first lines' texts, and for the label column the deepest indent
    let text_m = EguiMetrics { ctx: &ctx, mono_cells: false };
    let num_m = EguiMetrics { ctx: &ctx, mono_cells: true };
    let labels: Vec<String> = view.rows.iter().take(SAMPLE_ROWS).map(|r| r.label.clone()).collect();
    let deepest = view.rows.iter().map(|r| r.level).max().unwrap_or(0);
    let label_w = column_min_width(&view.label_header, "", "", &labels, &text_m) + deepest as f32 * theme::NESTED_INDENT_PX;
    let value_w: Vec<f32> = cols
        .iter()
        .map(|c| {
            let samples: Vec<String> = (0..n.min(SAMPLE_ROWS)).filter(|r| !view.rows[*r].is_blank()).map(|r| cell_text(c, r, col_formats, num_format, global_decimals)).collect();
            column_min_width(c.name().as_str(), "", "", &samples, &num_m)
        })
        .collect();

    let framed = egui::Frame::none().fill(tk.surface).stroke(theme::line(tk.border)).rounding(egui::Rounding::same(theme::RADIUS_PANEL)).show(ui, |ui| {
        let frame_inner = ui.max_rect();
        egui::ScrollArea::horizontal().id_salt(("nested_x", tab_id)).auto_shrink([false, true]).show(ui, |ui| {
            let limit = ui.clip_rect().intersect(frame_inner);
            let mut builder = TableBuilder::new(ui)
                .id_salt(("nested_grid", tab_id))
                .striped(false)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(label_w.max(theme::COLUMN_INITIAL_MIN_W).min(MAX_COLUMN_WIDTH + deepest as f32 * theme::NESTED_INDENT_PX)).at_least(label_w).clip(true).resizable(true));
            if let Some(y) = want_y {
                builder = builder.vertical_scroll_offset(y);
            }
            for w in &value_w {
                builder = builder.column(Column::initial((*w).max(theme::COLUMN_INITIAL_MIN_W).min(MAX_COLUMN_WIDTH)).at_least(*w).clip(true).resizable(true));
            }
            builder
                .header(theme::HEADER_ROW_H, |mut header| {
                    header.col(|ui| {
                        let rect = ui.max_rect();
                        theme::paint_header_bg(ui, rect, tk.header_bg, limit);
                        report.header.push((view.label_header.clone(), rect));
                        ui.add(egui::Label::new(theme::bold(view.label_header.clone()).color(tk.ink)).truncate());
                    });
                    for c in cols {
                        header.col(|ui| {
                            let rect = ui.max_rect();
                            theme::paint_header_bg(ui, rect, tk.header_bg, limit);
                            report.header.push((c.name().to_string(), rect));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(theme::bold(c.name().to_string()).color(tk.ink)).truncate());
                            });
                        });
                    }
                })
                .body(|body| {
                    body.rows(theme::DATA_ROW_H, n, |mut row| {
                        let i = row.index();
                        let line = &view.rows[i];
                        let bold = line.is_bold();
                        let mut entry = NestedLineReport { index: i, row: egui::Rect::NOTHING, label_cell: egui::Rect::NOTHING, label_text: None, cells: Vec::new() };
                        row.col(|ui| {
                            entry.label_cell = ui.max_rect();
                            entry.row = ui.max_rect();
                            if line.kind == NestedKind::Blank {
                                return;
                            }
                            // a rule above the Grand total, as the flat grid's total row has
                            if line.kind == NestedKind::GrandTotal {
                                let r = ui.max_rect().expand2(egui::vec2(ui.spacing().item_spacing.x / 2.0, ui.spacing().item_spacing.y / 2.0));
                                ui.painter().hline(r.x_range(), r.top(), theme::line(tk.border));
                            }
                            ui.add_space(line.level as f32 * theme::NESTED_INDENT_PX);
                            let text = if bold { theme::bold(line.label.clone()) } else { egui::RichText::new(line.label.clone()) };
                            let r = ui.add(egui::Label::new(text.color(tk.ink)).truncate().sense(egui::Sense::hover()));
                            entry.label_text = Some(r.rect);
                        });
                        for c in cols {
                            row.col(|ui| {
                                entry.row = entry.row.union(ui.max_rect());
                                if line.kind == NestedKind::Blank {
                                    return;
                                }
                                if line.kind == NestedKind::GrandTotal {
                                    let r = ui.max_rect().expand2(egui::vec2(ui.spacing().item_spacing.x / 2.0, ui.spacing().item_spacing.y / 2.0));
                                    ui.painter().hline(r.x_range(), r.top(), theme::line(tk.border));
                                }
                                let text = cell_text(c, i, col_formats, num_format, global_decimals);
                                let is_null = matches!(c.get(i), Ok(AnyValue::Null));
                                let rich = if bold { theme::mono_semibold(text.clone()) } else { theme::mono(text.clone()) }.color(if is_null { tk.muted } else { tk.ink });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let r = ui.add(egui::Label::new(rich).truncate().sense(egui::Sense::click()));
                                    // the text's own rectangle: what a click has to hit
                                    entry.cells.push((c.name().to_string(), r.rect));
                                    if r.double_clicked() && show_rows_off.is_none() {
                                        action = Some(GridAction::ShowRows(i, c.name().to_string()));
                                    }
                                    r.context_menu(|ui| {
                                        let item = ui.add_enabled(show_rows_off.is_none(), egui::Button::new("Show rows"));
                                        let item = match show_rows_off {
                                            Some(why) => item.on_disabled_hover_text(why),
                                            None => item.on_hover_text("Open the rows this line adds up in a new tab (or double-click the cell)"),
                                        };
                                        if item.clicked() {
                                            action = Some(GridAction::ShowRows(i, c.name().to_string()));
                                            ui.close_menu();
                                        }
                                    });
                                });
                            });
                        }
                        report.lines.push(entry);
                    });
                });
        });
    });
    report.frame = Some(framed.response.rect);
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("nested_grid_report", tab_id)), report));
    action
}
