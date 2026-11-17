//! Screen module `table` (moved out of main.rs by Phase 3 R1).

use crate::ui::prelude::*;
use micro_parquet::summary::{default_spec_for, format_summary_value};

/// Text widths measured with the fonts egui has loaded.
struct EguiMetrics<'a> {
    ctx: &'a egui::Context,
    /// Cells of this column are drawn in the mono font (numbers, dates, IDs).
    mono_cells: bool,
}

impl EguiMetrics<'_> {
    fn width(&self, text: &str, font: egui::FontId) -> f32 {
        self.ctx.fonts(|f| f.layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE).size().x)
    }
}

impl micro_parquet::table_layout::FontMetrics for EguiMetrics<'_> {
    fn header(&self, text: &str) -> f32 {
        self.width(text, egui::FontId::new(theme::TEXT_BASE, theme::family_bold()))
    }
    fn selector(&self, text: &str) -> f32 {
        self.width(text, egui::FontId::new(theme::TEXT_LABEL, theme::family_semibold()))
    }
    fn summary(&self, text: &str) -> f32 {
        self.width(text, egui::FontId::new(theme::TEXT_MONO, theme::family_mono_semibold()))
    }
    fn cell(&self, text: &str) -> f32 {
        if self.mono_cells {
            self.width(text, egui::FontId::new(theme::TEXT_MONO, egui::FontFamily::Monospace))
        } else {
            self.width(text, egui::FontId::new(theme::TEXT_BASE, egui::FontFamily::Proportional))
        }
    }
}

/// Where one cell of the grid's heading block was laid out and what it may paint (tests check that
/// neither leaves the table's frame).
#[derive(Clone, Copy, Debug)]
pub struct CellRect {
    pub layout: egui::Rect,
    pub clip: egui::Rect,
}

/// What the grid drew in its last frame: its frame, and the name, selector and summary cells of
/// its heading block, one of each per column.
#[derive(Clone, Debug, Default)]
pub struct GridReport {
    pub frame: Option<egui::Rect>,
    pub header: Vec<(String, CellRect)>,
    pub selector: Vec<(String, CellRect)>,
    pub summary: Vec<(String, CellRect)>,
}

/// The report of the grid drawn for `tab_id` in the last frame.
pub fn grid_report(ctx: &egui::Context, tab_id: u64) -> Option<GridReport> {
    ctx.data(|d| d.get_temp::<GridReport>(egui::Id::new(("grid_report", tab_id))))
}

/// Asks the grid of `tab_id` to scroll sideways to `x` in its next frame.
pub fn request_grid_scroll(ctx: &egui::Context, tab_id: u64, x: f32) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("grid_scroll_x", tab_id)), x));
}

/// Asks the grid of `tab_id` to scroll its rows to `y` in its next frame (the heading block stays).
pub fn request_grid_scroll_y(ctx: &egui::Context, tab_id: u64, y: f32) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("grid_scroll_y", tab_id)), y));
}

/// The summary row's data for the grid: the computed cells and the user's choices.
pub struct SummaryView<'a> {
    pub cells: &'a [micro_parquet::summary::SummaryCell],
    pub overrides: &'a HashMap<String, SummarySpec>,
    /// The summary job failed: the reason (every chosen summary shows "—").
    pub error: Option<&'a str>,
    /// The pinned control column: the default weight of rate-like columns.
    pub control_col: Option<&'a str>,
}

/// The text of one column's summary band and its hover text. `on` = a summary is chosen.
pub fn summary_band_text(
    sv: &SummaryView,
    name: &str,
    on: bool,
    spec_fmt: Option<&ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    decimals: usize,
) -> (String, Option<String>) {
    if let Some(e) = sv.error {
        return if on {
            ("\u{2014}".to_string(), Some(format!("The summary row could not be computed: {}", e)))
        } else {
            (String::new(), None)
        };
    }
    match sv.cells.iter().find(|x| x.column == name) {
        Some(cell) if cell.agg != SummaryAgg::None => (
            format_summary_value(cell, spec_fmt, num_format, decimals),
            cell.note.clone(),
        ),
        Some(_) => (String::new(), None),
        None if on && sv.cells.is_empty() => ("\u{2026}".to_string(), None),
        None => (String::new(), None),
    }
}

/// What the user did in the grid's header.
pub enum GridAction {
    /// Chose a summary (or its weight column) in the selector row.
    SetSummary(String, SummarySpec),
    /// Clicked the header: select the column.
    Select(String),
    /// Chose a sort from the header menu: (column, highest first, add as a further level).
    Sort(String, bool, bool),
    ClearSort,
}

/// How a column's cells are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CellKind {
    /// Numbers: right-aligned, mono.
    Number,
    /// Dates and number-like text (codes, references): left-aligned, mono.
    MonoText,
    /// Everything else: left-aligned, Source Sans 3.
    Text,
}

/// Mono (JetBrains Mono) is chosen by what a column holds, not by what it is called: Float, Int
/// and Date columns, and Text columns whose sampled values contain only digits, '-', '/', '.'
/// and spaces. All other text is Source Sans 3.
pub(crate) fn cell_kind(c: &polars::prelude::Series) -> CellKind {
    let dt = c.dtype();
    if dt.is_numeric() {
        CellKind::Number
    } else if dt.is_temporal() {
        CellKind::MonoText
    } else if let Ok(ca) = c.str() {
        let sample = (0..ca.len().min(micro_parquet::table_layout::SAMPLE_ROWS)).map(|i| ca.get(i));
        if micro_parquet::table_layout::text_column_is_mono(sample) {
            CellKind::MonoText
        } else {
            CellKind::Text
        }
    } else {
        CellKind::Text
    }
}

/// The text of one cell, as the grid shows it.
fn cell_text(
    c: &polars::prelude::Series,
    r_idx: usize,
    col_formats: &HashMap<String, ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    global_decimals: usize,
) -> String {
    let cname = c.name().as_str();
    let spec = col_formats.get(cname);
    match c.get(r_idx) {
        Ok(AnyValue::Null) => theme::BLANK_CELL.to_string(),
        Ok(AnyValue::Float64(v)) => format_metric_value(v, cname, spec, num_format, global_decimals),
        Ok(AnyValue::Float32(v)) => format_metric_value(v as f64, cname, spec, num_format, global_decimals),
        Ok(AnyValue::String(s)) => s.to_string(),
        Ok(AnyValue::Int64(v)) if !TabState::is_likely_id_column(cname) => format_with_commas(v as f64, 0),
        Ok(other) => format!("{}", other),
        Err(_) => "".to_string(),
    }
}

/// The summary choice shown for a column: the user's, else the default for its type.
fn effective_spec(c: &polars::prelude::Series, sv: &SummaryView) -> SummarySpec {
    sv.overrides
        .get(c.name().as_str())
        .cloned()
        .unwrap_or_else(|| default_spec_for(c.dtype(), c.name().as_str(), sv.control_col))
}

/// The selector (a small menu button) and the dark summary value under one column.
#[allow(clippy::too_many_arguments)]
fn draw_summary_cell(
    ui: &mut egui::Ui,
    c: &polars::prelude::Series,
    kind: CellKind,
    sv: &SummaryView,
    sel_rect: egui::Rect,
    band_rect: egui::Rect,
    tk: &theme::Tokens,
    spec_fmt: Option<&ColumnFormatSpec>,
    num_format: NumberDisplayFormat,
    decimals: usize,
    weight_guess: Option<&str>,
    action: &mut Option<GridAction>,
) {
    let name = c.name().to_string();
    let spec = effective_spec(c, sv);
    // selector row background and the selector chip
    ui.painter().rect_filled(sel_rect, 0.0, tk.bg);
    ui.painter().hline(sel_rect.x_range(), sel_rect.top() + 0.5, theme::line(tk.border));
    let on = spec.agg != SummaryAgg::None;
    let (fg, bg, stroke) = if on {
        (tk.selector_on_fg, tk.selector_on_bg, tk.selector_on_border)
    } else {
        (tk.selector_off_fg, egui::Color32::TRANSPARENT, tk.border)
    };
    let inner = sel_rect.shrink2(theme::SELECTOR_INSET);
    let layout = if kind == CellKind::Number {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(layout), |ui| {
        ui.spacing_mut().button_padding = theme::SELECTOR_BTN_PAD;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = bg;
        ui.visuals_mut().widgets.inactive.bg_stroke = theme::line(stroke);
        ui.visuals_mut().widgets.inactive.rounding = egui::Rounding::same(theme::RADIUS_SMALL);
        let label = format!("{} {}", spec.agg.label(), theme::ph::CARET_DOWN);
        ui.menu_button(theme::semibold(label).size(theme::TEXT_LABEL).color(fg), |ui| {
            let numeric = c.dtype().is_numeric();
            for agg in SummaryAgg::all() {
                let needs_numbers = matches!(agg, SummaryAgg::Sum | SummaryAgg::Average | SummaryAgg::WeightedAvg);
                let enabled = !needs_numbers || numeric;
                let resp = ui.add_enabled(enabled, egui::SelectableLabel::new(spec.agg == *agg, agg.label()));
                let resp = if enabled { resp } else { resp.on_disabled_hover_text(format!("{} needs a number column.", agg.label())) };
                if resp.clicked() {
                    let mut next = SummarySpec { agg: *agg, weight_col: spec.weight_col.clone() };
                    if *agg == SummaryAgg::WeightedAvg && next.weight_col.is_none() {
                        // weights are usually balances: the first column named like one
                        next.weight_col = weight_guess.filter(|w| *w != name).map(str::to_string);
                    }
                    *action = Some(GridAction::SetSummary(name.clone(), next));
                    ui.close_menu();
                }
            }
        });
    });

    // summary band
    ui.painter().rect_filled(band_rect, 0.0, tk.summary_bg);
    ui.painter().vline(band_rect.right() - 0.5, band_rect.y_range(), theme::line(tk.summary_divider));
    let (text, note) = summary_band_text(sv, &name, on, spec_fmt, num_format, decimals);
    let layout = if kind == CellKind::Number {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    let inner = band_rect.shrink2(theme::BAND_INSET);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner).layout(layout), |ui| {
        let r = ui.add(
            egui::Label::new(theme::mono_semibold(text).color(tk.summary_fg)).truncate(),
        );
        if let Some(n) = note {
            r.on_hover_text(n);
        }
    });
}

impl MicroParquetApp {
    /// Draws the page. Returns what the user did in the header, if anything.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_data_grid(
        ui: &mut egui::Ui,
        df: &DataFrame,
        col_formats: &HashMap<String, ColumnFormatSpec>,
        row_offset: usize,
        num_format: NumberDisplayFormat,
        global_decimals: usize,
        tab_id: u64,
        selected: Option<&str>,
        sort_marks: &HashMap<String, (usize, bool)>,
        summary: Option<&SummaryView>,
    ) -> Option<GridAction> {
        use micro_parquet::table_layout::{column_min_width, MAX_COLUMN_WIDTH, SAMPLE_ROWS};
        let tk = *theme::tokens(ui.visuals().dark_mode);
        let cols = df.get_columns();
        let num_rows = df.height();
        let ctx = ui.ctx().clone();
        let mut action: Option<GridAction> = None;
        let weight_guess = micro_parquet::tab_jobs::guess_balance_column(df);
        let shading: Option<Vec<Vec<micro_parquet::banking::Shade>>> = ui.ctx().data(|d| d.get_temp(egui::Id::new(("grid_shading", tab_id))));
        let mut report = GridReport::default();
        let want_x = ui.ctx().data_mut(|d| d.remove_temp::<f32>(egui::Id::new(("grid_scroll_x", tab_id))));
        let want_y = ui.ctx().data_mut(|d| d.remove_temp::<f32>(egui::Id::new(("grid_scroll_y", tab_id))));

        // Minimum widths: header, selector, summary and the first 100 rows' texts.
        let kinds: Vec<CellKind> = cols.iter().map(cell_kind).collect();
        let min_widths: Vec<f32> = cols
            .iter()
            .zip(&kinds)
            .map(|(c, kind)| {
                let m = EguiMetrics { ctx: &ctx, mono_cells: *kind != CellKind::Text };
                let samples: Vec<String> = (0..num_rows.min(SAMPLE_ROWS))
                    .map(|r| cell_text(c, r, col_formats, num_format, global_decimals))
                    .collect();
                let (sel, sum) = match summary {
                    Some(sv) => {
                        let spec = effective_spec(c, sv);
                        let text = sv
                            .cells
                            .iter()
                            .find(|x| x.column == c.name().as_str())
                            .map(|x| format_summary_value(x, col_formats.get(c.name().as_str()), num_format, global_decimals))
                            .unwrap_or_default();
                        (spec.agg.label().to_string(), text)
                    }
                    None => (String::new(), String::new()),
                };
                column_min_width(c.name().as_str(), &sel, &sum, &samples, &m)
            })
            .collect();
        let row_no_width = EguiMetrics { ctx: &ctx, mono_cells: true };
        let row_no_min = {
            use micro_parquet::table_layout::FontMetrics;
            row_no_width.cell(&format_with_commas((row_offset + num_rows.max(1)) as f64, 0)) + theme::ROW_NO_PAD
        };

        // One tall header row holds three bands: names (38), the summary selectors (30) and the
        // summary values (38), so every column's three cells line up.
        let head_height = if summary.is_some() {
            theme::HEADER_ROW_H + theme::SELECTOR_ROW_H + theme::SUMMARY_BAND_H
        } else {
            theme::HEADER_ROW_H
        };

        let framed = egui::Frame::none()
            .fill(tk.surface)
            .stroke(theme::line(tk.border))
            .rounding(egui::Rounding::same(theme::RADIUS_PANEL))
            .show(ui, |ui| {
                // The frame is as wide as the space it is given; what is wider than it scrolls
                // sideways inside it, heading block and rows together, and is clipped to it.
                // (egui clips a scroll area 3 px wider than it is; the frame's own edge is the limit)
                let frame_inner = ui.max_rect();
                let mut area = egui::ScrollArea::horizontal().id_salt(("grid_x", tab_id)).auto_shrink([false, true]);
                if let Some(x) = want_x {
                    area = area.horizontal_scroll_offset(x);
                }
                area.show(ui, |ui| {
                    // nothing of the heading block may be painted outside what the frame shows
                    let limit = ui.clip_rect().intersect(frame_inner);
                    // The id keeps each tab's dragged column widths apart.
                    let mut builder = TableBuilder::new(ui)
                        .id_salt(("data_grid", tab_id))
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::initial(row_no_min.max(theme::ROW_NO_COL_W)).at_least(row_no_min.max(theme::ROW_NO_MIN_W)));
                    if let Some(y) = want_y {
                        builder = builder.vertical_scroll_offset(y);
                    }
                    for w in &min_widths {
                        builder = builder.column(
                            Column::initial((*w).max(theme::COLUMN_INITIAL_MIN_W).min(MAX_COLUMN_WIDTH))
                                .at_least(*w)
                                .clip(true)
                                .resizable(true),
                        );
                    }
                    builder
                        .header(head_height, |mut header| {
                            header.col(|ui| {
                                let band = egui::Rect::from_min_size(
                                    ui.max_rect().min,
                                    egui::vec2(ui.max_rect().width(), theme::HEADER_ROW_H),
                                );
                                theme::paint_header_bg(ui, band, tk.header_bg, limit);
                                ui.label(theme::label_text("#").color(tk.muted));
                            });
                            for (c, kind) in cols.iter().zip(&kinds) {
                                header.col(|ui| {
                                    let cell_rect = ui.max_rect();
                                    let name = c.name().to_string();
                                    // The name band is the top 38 px of the cell.
                                    let rect = egui::Rect::from_min_size(
                                        cell_rect.min,
                                        egui::vec2(cell_rect.width(), theme::HEADER_ROW_H),
                                    );
                                    let is_sel = selected == Some(name.as_str());
                                    let resp = ui.interact(rect, ui.id().with(("hdr", name.as_str())), egui::Sense::click());
                                    // the heading row's background; the selected column's is accent_soft
                                    theme::paint_header_bg(ui, rect, if is_sel { tk.accent_soft } else { tk.header_bg }, limit);
                                    report.header.push((name.clone(), CellRect { layout: rect, clip: ui.clip_rect() }));
                                    if is_sel {
                                        ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, theme::line_thick(tk.accent));
                                    }
                                    let layout = if *kind == CellKind::Number {
                                        egui::Layout::right_to_left(egui::Align::Center)
                                    } else {
                                        egui::Layout::left_to_right(egui::Align::Center)
                                    };
                                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| {
                                        // The sort mark ("1 up-arrow") sits at the cell's edge.
                                        if let Some((level, desc)) = sort_marks.get(&name) {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} {}",
                                                    level,
                                                    if *desc { theme::ph::ARROW_DOWN } else { theme::ph::ARROW_UP }
                                                ))
                                                .size(theme::TEXT_LABEL)
                                                .color(tk.accent),
                                            );
                                        }
                                        ui.add(egui::Label::new(theme::bold(name.clone()).color(tk.ink)).truncate());
                                    });
                                    resp.clone().on_hover_text(format!("{}  \u{b7}  {:?}", name, c.dtype()));
                                    if resp.clicked() {
                                        action = Some(GridAction::Select(name.clone()));
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button("Sort lowest first").clicked() {
                                            action = Some(GridAction::Sort(name.clone(), false, false));
                                            ui.close_menu();
                                        }
                                        if ui.button("Sort highest first").clicked() {
                                            action = Some(GridAction::Sort(name.clone(), true, false));
                                            ui.close_menu();
                                        }
                                        if ui.button("Add as next sort level (lowest first)").clicked() {
                                            action = Some(GridAction::Sort(name.clone(), false, true));
                                            ui.close_menu();
                                        }
                                        if ui.button("Add as next sort level (highest first)").clicked() {
                                            action = Some(GridAction::Sort(name.clone(), true, true));
                                            ui.close_menu();
                                        }
                                        if !sort_marks.is_empty() && ui.button("Remove sort").clicked() {
                                            action = Some(GridAction::ClearSort);
                                            ui.close_menu();
                                        }
                                    });

                                    if let Some(sv) = summary {
                                        let sel_rect = egui::Rect::from_min_size(
                                            egui::pos2(cell_rect.min.x, cell_rect.min.y + theme::HEADER_ROW_H),
                                            egui::vec2(cell_rect.width(), theme::SELECTOR_ROW_H),
                                        );
                                        let band_rect = egui::Rect::from_min_size(
                                            egui::pos2(cell_rect.min.x, sel_rect.max.y),
                                            egui::vec2(cell_rect.width(), theme::SUMMARY_BAND_H),
                                        );
                                        draw_summary_cell(ui, c, *kind, sv, sel_rect, band_rect, &tk, col_formats.get(&name), num_format, global_decimals, weight_guess.as_deref(), &mut action);
                                        report.selector.push((name.clone(), CellRect { layout: sel_rect, clip: ui.clip_rect() }));
                                        report.summary.push((name.clone(), CellRect { layout: band_rect, clip: ui.clip_rect() }));
                                    }
                                });
                            }
                        })
                        .body(|body| {
                            body.rows(theme::DATA_ROW_H, num_rows, |mut row| {
                                let r_idx = row.index();
                                row.col(|ui| {
                                    ui.label(theme::mono(format_with_commas((row_offset + r_idx + 1) as f64, 0)).color(tk.muted));
                                });
                                for (c_idx, (c, kind)) in cols.iter().zip(&kinds).enumerate() {
                                    row.col(|ui| {
                                        // the CL migration matrix: an upgrade is green, a downgrade amber
                                        if let Some(fill) = shading.as_ref().and_then(|s| s.get(r_idx)).and_then(|r| r.get(c_idx)).and_then(|sh| theme::migration_fill(&tk, *sh)) {
                                            let cell = ui.max_rect().expand2(egui::vec2(ui.spacing().item_spacing.x / 2.0, ui.spacing().item_spacing.y / 2.0));
                                            ui.painter().rect_filled(cell, 0.0, fill);
                                        }
                                        let text = cell_text(c, r_idx, col_formats, num_format, global_decimals);
                                        // a blank cell shows a muted dash, never the word "null" (a text that
                                        // really says "null" is data and keeps its colour)
                                        let is_null = matches!(c.get(r_idx), Ok(AnyValue::Null));
                                        let rich = match kind {
                                            CellKind::Text => egui::RichText::new(text.clone()),
                                            _ => theme::mono(text.clone()),
                                        }
                                        .color(if is_null { tk.muted } else { tk.ink });
                                        let layout = if *kind == CellKind::Number {
                                            egui::Layout::right_to_left(egui::Align::Center)
                                        } else {
                                            egui::Layout::left_to_right(egui::Align::Center)
                                        };
                                        ui.with_layout(layout, |ui| {
                                            let r = ui.add(egui::Label::new(rich).truncate());
                                            // A cut cell shows its full text on hover.
                                            let cut = text.chars().count() > 6
                                                && r.rect.width() >= ui.max_rect().width() - 0.5;
                                            if cut {
                                                r.on_hover_text(text.clone());
                                            }
                                        });
                                    });
                                }
                            });
                        });
                });
            });
        report.frame = Some(framed.response.rect);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("grid_report", tab_id)), report));
        action
    }

    /// Filter chip bar (one chip per filter step) with the paging controls on the right.
    pub(crate) fn render_filter_bar(&mut self, ui: &mut egui::Ui) {
        let tk = *theme::tokens(self.dark_mode);
        let mut remove_cond: Option<(usize, usize)> = None;
        let mut edit_step: Option<usize> = None;
        let mut add_condition = false;
        let mut page_prev = false;
        let mut page_next = false;
        let mut page_first = false;
        let mut ctrl_changed = false;
        let mut picked_ctrl: Option<String> = None;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::semibold("Filters").size(theme::TEXT_SMALL).color(tk.muted));
                for (i, step) in tab.steps.iter().enumerate() {
                    let Some((conds, join, keep)) = step.as_filter_group() else {
                        continue;
                    };
                    for (j, c) in conds.iter().enumerate() {
                        if j > 0 {
                            ui.label(egui::RichText::new(join.label()).size(theme::TEXT_SMALL).color(tk.muted));
                        }
                        let text = if keep { c.describe() } else { format!("delete: {}", c.describe()) };
                        let (clicked, removed) = theme::chip(ui, &text);
                        if clicked {
                            edit_step = Some(i);
                        }
                        if removed {
                            remove_cond = Some((i, j));
                        }
                    }
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("+ Add condition").size(theme::TEXT_SMALL).color(tk.ink2))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(theme::line(tk.border))
                            .rounding(egui::Rounding::same(theme::RADIUS_CHIP)),
                    )
                    .clicked()
                {
                    add_condition = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_next = tab.viewport_offset + tab.viewport_limit < tab.total_rows;
                    if ui.add_enabled(can_next, egui::Button::new(theme::ph::CARET_RIGHT)).on_hover_text("Next page").clicked() {
                        page_next = true;
                    }
                    if ui.add_enabled(tab.viewport_offset > 0, egui::Button::new(theme::ph::CARET_LEFT)).on_hover_text("Previous page").clicked() {
                        page_prev = true;
                    }
                    if ui.add_enabled(tab.viewport_offset > 0, egui::Button::new(theme::ph::SKIP_BACK)).on_hover_text("First page").clicked() {
                        page_first = true;
                    }
                    ui.label(
                        egui::RichText::new(micro_parquet::table_layout::format_row_range(
                            tab.slice_offset + 1,
                            (tab.slice_offset + tab.cached_slice.height()).min(tab.total_rows),
                            tab.total_rows,
                        ))
                        .family(egui::FontFamily::Monospace)
                        .size(theme::TEXT_MONO)
                        .color(tk.ink),
                    );
                    ui.label(egui::RichText::new("Rows").color(tk.muted));
                    ui.separator();
                    let current_ctrl = tab.control_col.clone().unwrap_or_else(|| "None".to_string());
                    egui::ComboBox::from_id_salt("ctrl_col_selector")
                        .selected_text(current_ctrl)
                        .show_ui(ui, |ui| {
                            for col_series in tab.cached_slice.get_columns() {
                                let cname = col_series.name().to_string();
                                let on = tab.control_col.as_deref() == Some(cname.as_str());
                                if ui.selectable_label(on, &cname).clicked() {
                                    picked_ctrl = Some(cname);
                                }
                            }
                        });
                    ui.label(egui::RichText::new("Control column").color(tk.muted));
                });
            });
        }
        if let Some(c) = picked_ctrl {
            ctrl_changed = self.set_control_column(Some(c));
        }
        let mut refresh = ctrl_changed;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            if page_first {
                tab.viewport_offset = 0;
                refresh = true;
            }
            if page_prev {
                tab.viewport_offset = tab.viewport_offset.saturating_sub(tab.viewport_limit);
                refresh = true;
            }
            if page_next && tab.viewport_offset + tab.viewport_limit < tab.total_rows {
                tab.viewport_offset += tab.viewport_limit;
                refresh = true;
            }
        }
        if let Some((i, j)) = remove_cond {
            self.remove_filter_condition(i, j);
        }
        if let Some(i) = edit_step {
            self.open_filter_editor(Some(i), false);
        } else if add_condition {
            self.open_filter_editor_to_add();
        }
        if refresh {
            self.refresh_active_tab(ctrl_changed);
        }
        ui.add_space(theme::SPACE_M);
    }

    pub(crate) fn render_table_area(&mut self, ui: &mut egui::Ui) {
        // While a refresh runs, the last good page stays on screen, dimmed.
        let updating = self
            .tabs
            .get(self.active_tab)
            .is_some_and(|t| self.jobs.is_busy(JobKey::tab(JobKind::Refresh, t.id)));
        let mut action: Option<GridAction> = None;
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let marks = sort_marks(&tab.steps);
            let view = SummaryView {
                cells: &tab.summary_cells,
                overrides: &tab.summary,
                error: tab.summary_error.as_deref(),
                control_col: tab.control_col.as_deref(),
            };
            // A page that could not be read: the last one stays, dimmed and marked.
            if let Some(e) = &tab.page_error {
                ui.horizontal(|ui| {
                    theme::warning_label(ui, "Out of date");
                    ui.label(egui::RichText::new("The rows below are from an earlier page; the current one could not be read.").color(theme::tokens(self.dark_mode).muted))
                        .on_hover_text(e.as_str());
                })
                .response
                .on_hover_text(e.as_str());
            }
            let stale = tab.page_error.is_some();
            migration_shading_for(ui.ctx(), tab);
            ui.scope(|ui| {
                if updating || stale {
                    ui.set_opacity(theme::DISABLED_OPACITY);
                }
                action = Self::render_data_grid(
                    ui,
                    &tab.cached_slice,
                    &tab.col_formats,
                    tab.slice_offset,
                    self.num_format,
                    self.global_decimals,
                    tab.id,
                    self.selected_column.as_deref(),
                    &marks,
                    Some(&view),
                );
            });
        }
        match action {
            // Clicking a header selects the column and opens the Column tab.
            Some(GridAction::Select(c)) => self.select_column(&c),
            Some(GridAction::Sort(c, desc, add)) => self.sort_from_header(&c, desc, add),
            Some(GridAction::ClearSort) => self.clear_sort(),
            Some(GridAction::SetSummary(col, spec)) => self.set_summary(&col, spec),
            None => {}
        }
    }
}

/// The shading of the CL migration matrix of `tab`: for each cell of the page, an upgrade, a
/// downgrade or nothing (a cell without an amount is not shaded). Other tabs have none.
pub(crate) fn migration_shading_for(ctx: &egui::Context, tab: &TabState) {
    let id = egui::Id::new(("grid_shading", tab.id));
    if !matches!(tab.source, TabSource::ClMigration { .. }) {
        ctx.data_mut(|d| d.remove_temp::<Vec<Vec<micro_parquet::banking::Shade>>>(id));
        return;
    }
    let df = &tab.cached_slice;
    let Some(row_col) = df.get_columns().iter().position(|c| c.name().as_str() == micro_parquet::banking::MIGRATION_ROW_HEADING) else {
        return;
    };
    let labels: Vec<Option<String>> = match df.get_columns()[row_col].str() {
        Ok(s) => s.into_iter().map(|v| v.map(str::to_string)).collect(),
        Err(_) => return,
    };
    let mut shades: Vec<Vec<micro_parquet::banking::Shade>> = Vec::with_capacity(df.height());
    for (r, label) in labels.iter().enumerate() {
        let mut row = Vec::with_capacity(df.width());
        for c in df.get_columns() {
            let has_amount = match c.get(r) {
                Ok(AnyValue::Float64(x)) => x.abs() > 0.0,
                Ok(AnyValue::Int64(x)) => x != 0,
                _ => false,
            };
            row.push(if has_amount { micro_parquet::banking::migration_shade(label.as_deref().unwrap_or(""), c.name().as_str()) } else { micro_parquet::banking::Shade::None });
        }
        shades.push(row);
    }
    ctx.data_mut(|d| d.insert_temp(id, shades));
}
