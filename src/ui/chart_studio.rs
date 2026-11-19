//! The chart studio (Phase 7 C2): the Charts menu's dialogs and the studio controls beside the chart.
//! "New chart…" opens the gallery (cards grouped by the question they answer, and "Suggest a chart");
//! "Save chart…" and "Open saved chart…" keep named charts in `portable_data/charts/<name>.json`; the
//! extra studio controls (shape, titles, highlight, reference lines, measures, facet) are the sections
//! under the Phase 6 controls of the Chart tab.

use crate::ui::chart::ChartRects;
use crate::ui::prelude::*;
use micro_parquet::charts::gallery::{apply_card, card_by_id, cards_for, fit_columns, Card, Question};
use micro_parquet::charts::library;
use micro_parquet::charts::spec::{ChartAgg, ChartKind, ChartSpec, MeasureSpec, RefLine, Stack, Status, Transform};
use micro_parquet::charts::suggest::{looks_ordered, suggest, ColKind, SuggestCol, SuggestFlags, Suggestion};
use micro_parquet::charts::thumbnail::paint_thumbnail;

/// What the gallery drew (the render tests read it).
#[derive(Clone, Debug, Default)]
pub struct GalleryRects {
    pub window: Option<egui::Rect>,
    pub cards: Vec<(String, egui::Rect)>,
    pub columns: Vec<(String, egui::Rect)>,
    pub suggest: Option<egui::Rect>,
    pub use_suggestion: Option<egui::Rect>,
    pub waterfall_box: Option<egui::Rect>,
    pub close: Option<egui::Rect>,
}

/// State of the gallery dialog.
#[derive(Default)]
pub struct ChartGalleryState {
    /// The columns picked for "Suggest a chart".
    pub picked: Vec<String>,
    pub change_between_totals: bool,
    pub suggestion: Option<Result<Suggestion, String>>,
    pub rects: GalleryRects,
}

/// State of the "Save chart…" dialog.
#[derive(Default)]
pub struct ChartSaveState {
    pub name: String,
    pub confirm_replace: bool,
    pub message: Option<(bool, String)>,
    pub rects: Vec<(String, egui::Rect)>,
}

/// State of the "Open saved chart…" dialog.
#[derive(Default)]
pub struct ChartOpenState {
    pub items: Vec<(String, std::path::PathBuf)>,
    pub message: Option<(bool, String)>,
    pub confirm_delete: Option<String>,
    pub rects: Vec<(String, egui::Rect)>,
}

/// One saved chart in the chart book dialog.
#[derive(Clone, Debug)]
pub struct BookEntry {
    pub name: String,
    pub spec: Option<ChartSpec>,
    /// Why it cannot go in the book (columns the tab lacks, a damaged file).
    pub problem: Option<String>,
    pub checked: bool,
}

/// State of the "Chart book (HTML)…" dialog.
#[derive(Default)]
pub struct ChartBookState {
    pub entries: Vec<BookEntry>,
    pub title: String,
    pub message: Option<(bool, String)>,
    pub rects: Vec<(String, egui::Rect)>,
    /// Tests: write here instead of asking for a file.
    pub path_override: Option<std::path::PathBuf>,
}

fn is_number_dtype(dt: &polars::prelude::DataType) -> bool {
    dt.is_numeric() || matches!(dt, polars::prelude::DataType::Decimal(..))
}

impl MicroParquetApp {
    /// The columns of the active tab as the suggestion rules see them.
    pub(crate) fn chart_suggest_columns(&self) -> Vec<SuggestCol> {
        let Some(tab) = self.tabs.get(self.active_tab) else { return Vec::new() };
        tab.cached_slice
            .get_columns()
            .iter()
            .map(|c| {
                let name = c.name().to_string();
                let dt = c.dtype();
                let kind = if is_number_dtype(dt) {
                    ColKind::Number
                } else if matches!(dt, polars::prelude::DataType::Date | polars::prelude::DataType::Datetime(_, _)) {
                    ColKind::Date
                } else {
                    ColKind::Text
                };
                let ordered = kind == ColKind::Text && {
                    let sample: Vec<String> = c.iter().take(50).map(|v| v.get_str().map(str::to_string).unwrap_or_default()).collect();
                    looks_ordered(&name, &sample)
                };
                SuggestCol { name, kind, ordered }
            })
            .collect()
    }

    // ----------------------------------------------------------------------------- the gallery

    pub(crate) fn open_chart_gallery(&mut self) {
        self.chart_gallery = ChartGalleryState::default();
        self.active_tool = ActiveToolModal::ChartGallery;
    }

    /// Takes a card: the form, laid out as the card says, on the columns the tab has; the studio shows it.
    pub(crate) fn chart_use_card(&mut self, card: &Card) {
        self.chart_attach_tab_pub();
        let cols = self.chart_suggest_columns();
        let mut spec = self.chart.spec.clone();
        apply_card(&mut spec, card);
        fit_columns(&mut spec, &cols);
        self.chart_set_spec(spec);
        self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
        self.status_message = format!("Chart: {}. Change it in the Chart tab of the side panel.", card.name);
    }

    /// Takes a suggestion: its form and the columns that play each part.
    pub(crate) fn chart_use_suggestion(&mut self, s: &Suggestion) {
        self.chart_attach_tab_pub();
        let mut spec = self.chart.spec.clone();
        spec.kind = s.kind;
        spec.stack = s.stack;
        spec.horizontal = s.horizontal;
        spec.by_value = s.by_value;
        spec.stepped = false;
        spec.category = s.category.clone();
        spec.series = s.series.clone();
        spec.value = s.value.clone();
        spec.agg = if s.value.is_some() { ChartAgg::Sum } else { ChartAgg::Count };
        spec.weight = None;
        spec.measures.clear();
        spec.size = None;
        if s.kind == ChartKind::Heatmap {
            spec.measures = vec![MeasureSpec { column: spec.value.clone(), agg: spec.agg, weight: None, label: String::new(), transform: Transform::ShareOfSeries }];
        }
        if s.keep_order && !s.by_value && spec.order.is_natural() {
            // dates, months and the like sort naturally; an ordered text column keeps its natural order too
            spec.order = micro_parquet::sort_lists::SortOrder::Natural;
        }
        self.chart_set_spec(spec);
        self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
        self.status_message = format!("Chart: {} ({})", s.kind.label(), s.reason);
    }

    pub(crate) fn render_chart_gallery_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let dark = self.dark_mode;
        let mut close = false;
        let mut rects = GalleryRects::default();
        let mut take_card: Option<&'static Card> = None;
        let mut run_suggest = false;
        let mut use_suggestion = false;
        let columns = self.chart_suggest_columns();
        let mut picked = self.chart_gallery.picked.clone();
        let mut change = self.chart_gallery.change_between_totals;
        let suggestion = self.chart_gallery.suggestion.clone();
        let title = format!("{}  New chart", theme::ph::CHART_BAR);
        let out = theme::modal(&title).default_width(theme::CHART_GALLERY_W).show(ctx, |d| {
            d.body(|ui| {
                ui.label(egui::RichText::new("Pick the question your chart answers. Each card says when to use that chart and when not to.").color(tk.ink2));
                ui.add_space(theme::SPACE_S);
                // ------------------------------------------------ Suggest a chart
                ui.label(theme::semibold("Suggest a chart"));
                ui.label(egui::RichText::new("Pick the columns you want to chart; the studio proposes a form and says why.").size(theme::TEXT_LABEL).color(tk.muted));
                ui.horizontal_wrapped(|ui| {
                    for c in &columns {
                        let on = picked.contains(&c.name);
                        let kind = match c.kind {
                            ColKind::Number => "number",
                            ColKind::Date => "date",
                            ColKind::Text => {
                                if c.ordered {
                                    "ordered text"
                                } else {
                                    "text"
                                }
                            }
                        };
                        let r = ui.selectable_label(on, format!("{} \u{b7} {}", c.name, kind));
                        rects.columns.push((c.name.clone(), r.rect));
                        if r.clicked() {
                            if on {
                                picked.retain(|p| *p != c.name);
                            } else {
                                picked.push(c.name.clone());
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let r = ui.checkbox(&mut change, "A signed change between two totals (a bridge)");
                    rects.waterfall_box = Some(r.rect);
                    let b = ui.button("Suggest");
                    rects.suggest = Some(b.rect);
                    if b.clicked() {
                        run_suggest = true;
                    }
                });
                match &suggestion {
                    Some(Ok(s)) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(theme::semibold(format!("{}.", s.kind.label())));
                            ui.label(egui::RichText::new(&s.reason).color(tk.ink2));
                            let b = ui.button("Use this chart");
                            rects.use_suggestion = Some(b.rect);
                            if b.clicked() {
                                use_suggestion = true;
                            }
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(egui::RichText::new(e).color(tk.danger));
                    }
                    None => {}
                }
                ui.add_space(theme::SPACE_M);
                // ------------------------------------------------ the cards, by question
                for q in Question::all() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(theme::semibold(q.title()));
                        ui.label(egui::RichText::new(q.asks()).color(tk.muted));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(theme::SPACE_M, theme::SPACE_M);
                        for card in cards_for(*q) {
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(theme::CHART_CARD_W, theme::CHART_CARD_H), egui::Sense::click());
                            let hover = resp.hovered();
                            ui.painter().rect(rect, theme::RADIUS_PANEL, tk.surface, egui::Stroke::new(theme::CHART_GRID_PX, if hover { tk.accent } else { tk.border }));
                            let thumb = egui::Rect::from_min_size(rect.min + egui::vec2(theme::SPACE_S, theme::SPACE_S), egui::vec2(rect.width() - 2.0 * theme::SPACE_S, theme::CHART_THUMB_H));
                            paint_thumbnail(&ui.painter_at(rect), thumb, card.kind, card.stack, card.horizontal, dark);
                            let mut y = thumb.bottom() + theme::SPACE_S;
                            let text_w = rect.width() - 2.0 * theme::SPACE_S;
                            let name = ui.painter().layout(card.name.to_string(), egui::FontId::new(theme::TEXT_BASE, theme::family_semibold()), tk.ink, text_w);
                            let nh = name.size().y;
                            ui.painter().galley(egui::pos2(rect.left() + theme::SPACE_S, y), name, tk.ink);
                            y += nh + theme::SPACE_XS;
                            for (lead, text) in [("Use it when ", card.use_when), ("Avoid it when ", card.avoid_when)] {
                                let g = ui.painter().layout(format!("{}{}", lead, text), egui::FontId::new(theme::TEXT_LABEL, egui::FontFamily::Proportional), tk.ink2, text_w);
                                let gh = g.size().y;
                                ui.painter().galley(egui::pos2(rect.left() + theme::SPACE_S, y), g, tk.ink2);
                                y += gh + theme::SPACE_XS;
                            }
                            rects.cards.push((card.id.to_string(), rect));
                            if card.kind.html_only() {
                                ui.painter().text(egui::pos2(rect.right() - theme::SPACE_S, rect.top() + theme::SPACE_S), egui::Align2::RIGHT_TOP, "HTML", egui::FontId::new(theme::TEXT_LABEL, egui::FontFamily::Proportional), tk.muted);
                            }
                            if resp.clicked() {
                                take_card = Some(card);
                            }
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(theme::semibold("Banking templates"));
                    ui.label(egui::RichText::new("On a tool's result tab, Charts \u{25b8} \"Chart this result\" opens the studio with the template for that tool.").color(tk.muted));
                });
            });
            d.footer(|ui| {
                let b = ui.button("Close");
                rects.close = Some(b.rect);
                if b.clicked() {
                    close = true;
                }
            });
        });
        rects.window = out.map(|o| o.response.rect);
        self.chart_gallery.picked = picked;
        self.chart_gallery.change_between_totals = change;
        if run_suggest {
            let cols: Vec<SuggestCol> = self.chart_gallery.picked.iter().filter_map(|n| columns.iter().find(|c| c.name == *n).cloned()).collect();
            self.chart_gallery.suggestion = Some(suggest(&cols, SuggestFlags { change_between_totals: change }));
        }
        if use_suggestion {
            if let Some(Ok(s)) = self.chart_gallery.suggestion.clone() {
                self.chart_use_suggestion(&s);
                close = true;
            }
        }
        if let Some(card) = take_card {
            self.chart_use_card(card);
            close = true;
        }
        self.chart_gallery.rects = rects;
        ModalOutcome { close, step: None }
    }

    // ---------------------------------------------------------------------- save and open

    pub(crate) fn charts_dir(&self) -> std::path::PathBuf {
        self.paths.root_dir.join("charts")
    }

    pub(crate) fn open_chart_save(&mut self) {
        self.chart_attach_tab_pub();
        let name = match self.chart.data.as_ref() {
            Some(d) if self.chart.spec.title.trim().is_empty() && self.chart.spec.takeaway.trim().is_empty() => micro_parquet::charts::layout::auto_title(d),
            _ => {
                if !self.chart.spec.takeaway.trim().is_empty() {
                    self.chart.spec.takeaway.clone()
                } else {
                    self.chart.spec.title.clone()
                }
            }
        };
        self.chart_save = ChartSaveState { name, ..Default::default() };
        self.active_tool = ActiveToolModal::ChartSave;
    }

    pub(crate) fn render_chart_save_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        let mut save = false;
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let mut name = self.chart_save.name.clone();
        let confirm = self.chart_save.confirm_replace;
        let message = self.chart_save.message.clone();
        let dir = self.charts_dir();
        let title = format!("{}  Save chart", theme::ph::FLOPPY_DISK);
        theme::modal(&title).default_width(theme::DIALOG_W_SORT).show(ctx, |d| {
            d.body(|ui| {
                ui.label(format!("Saved in {}", dir.display()));
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(theme::PDF_TITLE_FIELD_W));
                    rects.push(("Name".to_string(), r.rect));
                });
                if confirm {
                    ui.label(egui::RichText::new("A chart with this name is saved already. Save again to replace it.").color(tk.warning));
                }
                if let Some((bad, m)) = &message {
                    ui.label(egui::RichText::new(m).color(if *bad { tk.danger } else { tk.success }));
                }
            });
            d.footer(|ui| {
                ui.horizontal(|ui| {
                    let b = theme::primary_button(ui, if confirm { "Replace" } else { "Save" });
                    rects.push(("Save".to_string(), b.rect));
                    if b.clicked() {
                        save = true;
                    }
                    let b = ui.button("Cancel");
                    rects.push(("Cancel".to_string(), b.rect));
                    if b.clicked() {
                        close = true;
                    }
                });
            });
        });
        if name != self.chart_save.name {
            self.chart_save.name = name.clone();
            self.chart_save.confirm_replace = false;
        }
        if save {
            if library::exists(&dir, &name) && !self.chart_save.confirm_replace {
                self.chart_save.confirm_replace = true;
            } else {
                match library::save_chart(&dir, &name, &self.chart.spec) {
                    Ok(p) => {
                        self.status_message = format!("Chart saved: {}", p.display());
                        close = true;
                    }
                    Err(e) => self.chart_save.message = Some((true, format!("{:#}", e))),
                }
            }
        }
        self.chart_save.rects = rects;
        ModalOutcome { close, step: None }
    }

    pub(crate) fn open_chart_open(&mut self) {
        self.chart_open = ChartOpenState { items: library::list_charts(&self.charts_dir()), ..Default::default() };
        self.active_tool = ActiveToolModal::ChartOpen;
    }

    /// Opens a saved chart on the active tab; a chart that uses columns the tab does not have is refused, naming them.
    pub(crate) fn chart_open_named(&mut self, path: &std::path::Path) -> Result<String, String> {
        let (name, spec) = library::load_chart(path).map_err(|e| format!("{:#}", e))?;
        let avail = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let missing = library::missing_columns(&spec, &avail);
        if !missing.is_empty() {
            return Err(format!("The chart '{}' uses column(s) this tab does not have: {}. Open it on the tab it was made for.", name, missing.join(", ")));
        }
        self.chart_attach_tab_pub();
        self.chart_set_spec(spec);
        self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
        Ok(name)
    }

    pub(crate) fn render_chart_open_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        let mut open: Option<std::path::PathBuf> = None;
        let mut delete: Option<(String, std::path::PathBuf)> = None;
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let items = self.chart_open.items.clone();
        let message = self.chart_open.message.clone();
        let dir = self.charts_dir();
        let title = format!("{}  Open saved chart", theme::ph::FOLDER_OPEN);
        theme::modal(&title).default_width(theme::DIALOG_W_SORT).show(ctx, |d| {
            d.body(|ui| {
                ui.label(egui::RichText::new(format!("Charts in {}", dir.display())).color(tk.muted));
                if items.is_empty() {
                    ui.label("No saved chart yet. Make one in the Chart tab and choose \"Save chart\u{2026}\".");
                }
                for (name, path) in &items {
                    ui.horizontal(|ui| {
                        let b = ui.button("Open");
                        rects.push((format!("Open {name}"), b.rect));
                        if b.clicked() {
                            open = Some(path.clone());
                        }
                        let b = ui.button("Delete");
                        rects.push((format!("Delete {name}"), b.rect));
                        if b.clicked() {
                            delete = Some((name.clone(), path.clone()));
                        }
                        ui.label(name);
                    });
                }
                if let Some(n) = &self.chart_open.confirm_delete {
                    ui.label(egui::RichText::new(format!("Delete '{}'? Click Delete again to confirm.", n)).color(tk.warning));
                }
                if let Some((bad, m)) = &message {
                    ui.add(egui::Label::new(egui::RichText::new(m).color(if *bad { tk.danger } else { tk.success })).wrap());
                }
            });
            d.footer(|ui| {
                let b = ui.button("Close");
                rects.push(("Close".to_string(), b.rect));
                if b.clicked() {
                    close = true;
                }
            });
        });
        if let Some((name, path)) = delete {
            if self.chart_open.confirm_delete.as_deref() == Some(name.as_str()) {
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        self.chart_open.items = library::list_charts(&dir);
                        self.chart_open.message = Some((false, format!("Deleted '{}'.", name)));
                    }
                    Err(e) => self.chart_open.message = Some((true, format!("Could not delete {}: {}", path.display(), e))),
                }
                self.chart_open.confirm_delete = None;
            } else {
                self.chart_open.confirm_delete = Some(name);
            }
        }
        if let Some(path) = open {
            match self.chart_open_named(&path) {
                Ok(name) => {
                    self.status_message = format!("Chart '{}' opened in the Chart tab.", name);
                    close = true;
                }
                Err(e) => self.chart_open.message = Some((true, e)),
            }
        }
        self.chart_open.rects = rects;
        ModalOutcome { close, step: None }
    }

    // ------------------------------------------------------------------------------- chart book

    /// Charts ▸ "Chart book (HTML)…": the saved charts that fit the active tab, ticked, then one page.
    pub(crate) fn open_chart_book(&mut self) {
        let avail = self.tabs.get(self.active_tab).map(|t| t.available_columns()).unwrap_or_default();
        let entries = library::list_charts(&self.charts_dir())
            .into_iter()
            .map(|(name, path)| match library::load_chart(&path) {
                Ok((_, spec)) => {
                    let missing = library::missing_columns(&spec, &avail);
                    let problem = (!missing.is_empty()).then(|| format!("this tab has no column {}", missing.join(", ")));
                    BookEntry { name, checked: problem.is_none(), spec: Some(spec), problem }
                }
                Err(e) => BookEntry { name, spec: None, problem: Some(format!("{:#}", e)), checked: false },
            })
            .collect();
        self.chart_book = ChartBookState { entries, title: "Charts".to_string(), ..Default::default() };
        self.active_tool = ActiveToolModal::ChartBook;
    }

    /// The charts ticked in the book dialog, each with how its numbers are written on the active tab.
    pub(crate) fn chart_book_selection(&self) -> Vec<(String, ChartSpec, micro_parquet::charts::layout::LayoutOpts)> {
        self.chart_book
            .entries
            .iter()
            .filter(|e| e.checked && e.problem.is_none())
            .filter_map(|e| {
                let spec = e.spec.clone()?;
                let scale = self.chart_scale(&spec);
                let opts = micro_parquet::charts::layout::LayoutOpts {
                    dark: false,
                    divisor: scale.map(|s| s.divisor).unwrap_or(1.0),
                    decimals: self.global_decimals,
                    unit: scale.as_ref().map(crate::ui::chart::scale_word).unwrap_or("").to_string(),
                    bare: false,
                    colours: Some(spec.colour_map.clone()),
                };
                Some((e.name.clone(), spec, opts))
            })
            .collect()
    }

    /// Sends the book to the worker: it calculates every ticked chart over the whole tab and writes the page.
    pub(crate) fn chart_book_write_to(&mut self, path: std::path::PathBuf) -> Result<(), String> {
        let charts = self.chart_book_selection();
        if charts.is_empty() {
            return Err("Tick at least one chart for the book.".to_string());
        }
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Err("Open the tab the charts are made for first.".to_string());
        };
        let exec = tab.exec_steps.min(tab.steps.len());
        let req = micro_parquet::tab_jobs::ChartBookRequest {
            source: tab.source.clone(),
            steps: tab.steps[..exec].to_vec(),
            temp_dir: self.paths.temp_dir.clone(),
            charts,
            title: if self.chart_book.title.trim().is_empty() { "Charts".to_string() } else { self.chart_book.title.trim().to_string() },
            date: chrono::Local::now().format("%-d %B %Y").to_string(),
            path,
        };
        self.jobs.submit(JobKey::unique(JobKind::Export), "Writing the chart book", true, move |ctx| micro_parquet::tab_jobs::chart_book_job(req, ctx));
        Ok(())
    }

    pub(crate) fn render_chart_book_modal(&mut self, ctx: &egui::Context, _cols: Vec<String>) -> ModalOutcome {
        let tk = *theme::tokens(self.dark_mode);
        let mut close = false;
        let mut write = false;
        let mut rects: Vec<(String, egui::Rect)> = Vec::new();
        let mut entries = self.chart_book.entries.clone();
        let mut title = self.chart_book.title.clone();
        let message = self.chart_book.message.clone();
        let dir = self.charts_dir();
        let any = entries.iter().any(|e| e.checked && e.problem.is_none());
        let heading = format!("{}  Chart book (HTML)", theme::ph::FILES);
        theme::modal(&heading).default_width(theme::DIALOG_W_SORT).show(ctx, |d| {
            d.body(|ui| {
                ui.label(egui::RichText::new("One web page with several of your saved charts, calculated on the active tab. Saved charts are in the charts folder:").color(tk.muted));
                ui.label(egui::RichText::new(dir.display().to_string()).color(tk.muted));
                ui.horizontal(|ui| {
                    ui.label("Title");
                    let r = ui.add(egui::TextEdit::singleline(&mut title).desired_width(theme::CHART_REF_LABEL_W * 2.0));
                    rects.push(("Title".to_string(), r.rect));
                });
                if entries.is_empty() {
                    ui.label("No saved chart yet. Make one in the Chart tab and choose \"Save chart\u{2026}\".");
                }
                for (i, e) in entries.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let r = ui.add_enabled(e.problem.is_none(), egui::Checkbox::new(&mut e.checked, &e.name));
                        rects.push((format!("Chart {i}"), r.rect));
                        if let Some(p) = &e.problem {
                            ui.label(egui::RichText::new(format!("not used: {p}")).size(theme::TEXT_LABEL).color(tk.warning));
                        }
                    });
                }
                if let Some((bad, m)) = &message {
                    ui.add(egui::Label::new(egui::RichText::new(m).color(if *bad { tk.danger } else { tk.success })).wrap());
                }
            });
            d.footer(|ui| {
                let b = ui.add_enabled(any, egui::Button::new("Write chart book\u{2026}"));
                rects.push(("Write chart book".to_string(), b.rect));
                if b.clicked() {
                    write = true;
                }
                let b = ui.button("Close");
                rects.push(("Close".to_string(), b.rect));
                if b.clicked() {
                    close = true;
                }
            });
        });
        self.chart_book.entries = entries;
        self.chart_book.title = title;
        if write {
            let name = format!("{}.html", micro_parquet::charts::echarts::file_stem(&self.chart_book.title));
            let picked = match self.chart_book.path_override.clone() {
                Some(p) => Some(p),
                None => rfd::FileDialog::new().add_filter("HTML page", &["html"]).set_file_name(&name).set_title("Save the chart book").save_file(),
            };
            if let Some(path) = picked {
                match self.chart_book_write_to(path) {
                    Ok(()) => {
                        self.status_message = "Writing the chart book\u{2026}".to_string();
                        close = true;
                    }
                    Err(e) => self.chart_book.message = Some((true, e)),
                }
            }
        }
        self.chart_book.rects = rects;
        ModalOutcome { close, step: None }
    }

    // ---------------------------------------------------------------------- studio controls

    /// The studio controls under the Phase 6 ones: shape, then Titles and look, Reference lines and
    /// Measures and panels (each a collapsing section). Changes go into `spec`; the caller saves them.
    pub(crate) fn studio_sections(&mut self, ui: &mut egui::Ui, spec: &mut ChartSpec, avail: &[String], numeric: &[String], rects: &mut ChartRects) {
        let tk = *theme::tokens(self.dark_mode);
        let names: Vec<String> = match self.chart.data.as_ref() {
            Some(d) if spec.series.is_some() => d.series.iter().filter(|s| !s.is_empty()).cloned().collect(),
            Some(d) => d.categories.clone(),
            None => Vec::new(),
        };
        let combo_w = theme::CHART_COMBO_W;
        // ---- the shape of the chosen form
        let shaped = matches!(spec.kind, ChartKind::Bar | ChartKind::Area | ChartKind::Line | ChartKind::Histogram | ChartKind::Bubble | ChartKind::Waterfall);
        if shaped {
            egui::Grid::new("chart_shape").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                if spec.kind == ChartKind::Bar {
                    ui.label("Layout");
                    let r = egui::ComboBox::from_id_salt("chart_layout").width(combo_w).selected_text(if spec.horizontal { "Bars (horizontal)" } else { "Columns" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut spec.horizontal, false, "Columns");
                        ui.selectable_value(&mut spec.horizontal, true, "Bars (horizontal)");
                    });
                    rects.fields.push(("Layout".to_string(), r.response.rect));
                    ui.end_row();
                }
                if matches!(spec.kind, ChartKind::Bar | ChartKind::Area) {
                    ui.label("Series");
                    let text = match spec.stack {
                        Stack::Grouped => "Side by side",
                        Stack::Stacked => "Stacked",
                        Stack::Percent => "100% stacked",
                    };
                    let r = egui::ComboBox::from_id_salt("chart_stack").width(combo_w).selected_text(text).show_ui(ui, |ui| {
                        ui.selectable_value(&mut spec.stack, Stack::Grouped, "Side by side");
                        ui.selectable_value(&mut spec.stack, Stack::Stacked, "Stacked");
                        ui.selectable_value(&mut spec.stack, Stack::Percent, "100% stacked");
                    });
                    rects.fields.push(("Stack".to_string(), r.response.rect));
                    ui.end_row();
                }
                if spec.kind == ChartKind::Line {
                    ui.label("Line");
                    let r = ui.checkbox(&mut spec.stepped, "Stepped");
                    rects.fields.push(("Stepped".to_string(), r.rect));
                    ui.end_row();
                }
                if spec.kind == ChartKind::Histogram {
                    ui.label("Bins");
                    let mut bins = spec.bins.unwrap_or(micro_parquet::column_stats::HISTOGRAM_BINS);
                    let r = ui.add(egui::DragValue::new(&mut bins).range(2..=100).speed(0.2)); // not a size: a drag speed
                    rects.fields.push(("Bins".to_string(), r.rect));
                    spec.bins = Some(bins);
                    ui.end_row();
                }
                if spec.kind == ChartKind::Bubble {
                    ui.label("Size");
                    let shown = spec.size.as_ref().and_then(|m| m.column.clone()).unwrap_or_else(|| "(choose)".to_string());
                    let r = egui::ComboBox::from_id_salt("chart_size").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                        for c in numeric {
                            if ui.selectable_label(spec.size.as_ref().and_then(|m| m.column.as_ref()) == Some(c), c).clicked() {
                                spec.size = Some(MeasureSpec { column: Some(c.clone()), ..Default::default() });
                            }
                        }
                    });
                    rects.fields.push(("Size".to_string(), r.response.rect));
                    ui.end_row();
                }
                if spec.kind == ChartKind::Waterfall {
                    ui.label("Steps");
                    let text = match spec.waterfall {
                        micro_parquet::charts::spec::WaterfallMode::StartStepsEnd => "First = start, last = end",
                        micro_parquet::charts::spec::WaterfallMode::StartSteps => "First = start, end calculated",
                    };
                    let r = egui::ComboBox::from_id_salt("chart_wf").width(combo_w).selected_text(text).show_ui(ui, |ui| {
                        ui.selectable_value(&mut spec.waterfall, micro_parquet::charts::spec::WaterfallMode::StartStepsEnd, "First = start, last = end");
                        ui.selectable_value(&mut spec.waterfall, micro_parquet::charts::spec::WaterfallMode::StartSteps, "First = start, end calculated");
                    });
                    rects.fields.push(("Waterfall steps".to_string(), r.response.rect));
                    ui.end_row();
                }
            });
        }
        // ---- titles and look
        let h = egui::CollapsingHeader::new("Titles and look").id_salt("chart_look").show(ui, |ui| {
            egui::Grid::new("chart_look_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                for (label, key, field) in [("Title", "Title", &mut spec.title), ("Subtitle", "Subtitle", &mut spec.subtitle), ("Takeaway", "Takeaway", &mut spec.takeaway), ("Footnote", "Footnote", &mut spec.footnote)] {
                    ui.label(label);
                    let hint = if key == "Takeaway" { "replaces the title" } else { "automatic" };
                    let r = ui.add(egui::TextEdit::singleline(field).hint_text(hint).desired_width(theme::CHART_TEXT_FIELD_W));
                    rects.fields.push((key.to_string(), r.rect));
                    ui.end_row();
                }
                ui.label("Highlight");
                let shown = spec.highlight.clone().unwrap_or_else(|| "(none)".to_string());
                let r = egui::ComboBox::from_id_salt("chart_highlight").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut spec.highlight, None, "(none)");
                    for n in names.iter().take(60) {
                        ui.selectable_value(&mut spec.highlight, Some(n.clone()), n);
                    }
                });
                rects.fields.push(("Highlight".to_string(), r.response.rect));
                ui.end_row();
            });
            ui.label(egui::RichText::new("The takeaway is your sentence for the title. The rest of the chart is de-emphasised around the highlight.").size(theme::TEXT_LABEL).color(tk.muted));
        });
        rects.fields.push(("Titles and look".to_string(), h.header_response.rect));
        // ---- reference lines
        let h = egui::CollapsingHeader::new(format!("Reference lines ({})", spec.ref_lines.len())).id_salt("chart_refs").show(ui, |ui| {
            let mut remove: Option<usize> = None;
            for (i, rl) in spec.ref_lines.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    let r = ui.add(egui::DragValue::new(&mut rl.value).speed(0.1)); // not a size: a drag speed
                    rects.fields.push((format!("Line {i} value"), r.rect));
                    let r = ui.add(egui::TextEdit::singleline(&mut rl.label).hint_text("label").desired_width(theme::CHART_REF_LABEL_W));
                    rects.fields.push((format!("Line {i} label"), r.rect));
                    let r = egui::ComboBox::from_id_salt(("chart_ref_status", i)).width(theme::CHART_COMBO_NARROW_W).selected_text(rl.status.label()).show_ui(ui, |ui| {
                        for s in Status::all() {
                            ui.selectable_value(&mut rl.status, *s, s.label());
                        }
                    });
                    rects.fields.push((format!("Line {i} status"), r.response.rect));
                    let r = ui.button("Remove");
                    rects.fields.push((format!("Line {i} remove"), r.rect));
                    if r.clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                spec.ref_lines.remove(i);
            }
            let b = ui.button("Add reference line");
            rects.fields.push(("Add reference line".to_string(), b.rect));
            if b.clicked() {
                spec.ref_lines.push(RefLine { value: 0.0, label: String::new(), status: Status::Critical, panel: 0 });
            }
            ui.label(egui::RichText::new("A limit, minimum or outlier line: solid, in its status colour, with the status written beside it.").size(theme::TEXT_LABEL).color(tk.muted));
        });
        rects.fields.push(("Reference lines".to_string(), h.header_response.rect));
        // ---- measures, panels and the facet
        let h = egui::CollapsingHeader::new(format!("Measures and panels ({})", spec.measure_list().len())).id_salt("chart_measures").show(ui, |ui| {
            egui::Grid::new("chart_measures_grid").num_columns(2).spacing(egui::vec2(theme::SPACE_M, theme::SPACE_XS)).show(ui, |ui| {
                ui.label("Show as");
                let cur = spec.measures.first().map(|m| m.transform).unwrap_or(Transform::None);
                let mut now = cur;
                let r = egui::ComboBox::from_id_salt("chart_transform").width(combo_w).selected_text(cur.label()).show_ui(ui, |ui| {
                    for t in [Transform::None, Transform::ShareOfTotal, Transform::ShareOfCategory, Transform::ShareOfSeries, Transform::RunningSum] {
                        ui.selectable_value(&mut now, t, t.label());
                    }
                });
                rects.fields.push(("Show as".to_string(), r.response.rect));
                if now != cur {
                    if spec.measures.is_empty() {
                        spec.measures = spec.measure_list();
                    }
                    spec.measures[0].transform = now;
                }
                ui.end_row();
                ui.label("Small multiples");
                let shown = spec.facet.clone().unwrap_or_else(|| "(none)".to_string());
                let r = egui::ComboBox::from_id_salt("chart_facet").width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut spec.facet, None, "(none)");
                    for c in avail.iter().filter(|c| **c != spec.category && spec.series.as_deref() != Some(c.as_str())) {
                        ui.selectable_value(&mut spec.facet, Some(c.clone()), c);
                    }
                });
                rects.fields.push(("Facet".to_string(), r.response.rect));
                ui.end_row();
            });
            // extra measures: each its own panel on the shared x-axis (never a second axis)
            let mut remove: Option<usize> = None;
            let n_extra = spec.measures.len().saturating_sub(1);
            for k in 0..n_extra {
                let m = &mut spec.measures[k + 1];
                ui.horizontal_wrapped(|ui| {
                    let shown = m.column.clone().unwrap_or_else(|| "(choose)".to_string());
                    let r = egui::ComboBox::from_id_salt(("chart_m_col", k)).width(combo_w).selected_text(shown).show_ui(ui, |ui| {
                        for c in avail {
                            ui.selectable_value(&mut m.column, Some(c.clone()), c);
                        }
                    });
                    rects.fields.push((format!("Measure {} column", k + 2), r.response.rect));
                    let r = egui::ComboBox::from_id_salt(("chart_m_agg", k)).width(theme::CHART_COMBO_NARROW_W).selected_text(m.agg.label()).show_ui(ui, |ui| {
                        for a in ChartAgg::all() {
                            ui.selectable_value(&mut m.agg, *a, a.label());
                        }
                    });
                    rects.fields.push((format!("Measure {} aggregation", k + 2), r.response.rect));
                    let r = ui.button("Remove");
                    rects.fields.push((format!("Measure {} remove", k + 2), r.rect));
                    if r.clicked() {
                        remove = Some(k + 1);
                    }
                });
            }
            if let Some(i) = remove {
                spec.measures.remove(i);
                if spec.measures.len() == 1 && spec.measures[0].transform == Transform::None {
                    spec.measures.clear();
                }
            }
            let b = ui.add_enabled(spec.measure_list().len() < micro_parquet::charts::tokens::MAX_PANELS, egui::Button::new("Add measure"));
            rects.fields.push(("Add measure".to_string(), b.rect));
            if b.clicked() {
                if spec.measures.is_empty() {
                    spec.measures = spec.measure_list();
                }
                let col = numeric.iter().find(|c| spec.value.as_ref() != Some(*c)).cloned().or_else(|| numeric.first().cloned());
                spec.measures.push(MeasureSpec { column: col, agg: ChartAgg::Sum, ..Default::default() });
            }
            ui.label(egui::RichText::new("Two measures of different scale become linked panels on one shared x-axis: there is no dual axis.").size(theme::TEXT_LABEL).color(tk.muted));
        });
        rects.fields.push(("Measures and panels".to_string(), h.header_response.rect));
        // the primary measure's column, aggregation and weight stay what the controls above say
        if !spec.measures.is_empty() {
            spec.measures[0].column = spec.value.clone();
            spec.measures[0].agg = spec.agg;
            spec.measures[0].weight = spec.weight.clone();
        }
        // ---- advanced options: JSON merged over the ECharts option of the HTML page
        let label = if spec.advanced_json.trim().is_empty() { "Advanced options (HTML only)".to_string() } else { "Advanced options (HTML only, set)".to_string() };
        let h = egui::CollapsingHeader::new(label).id_salt("chart_advanced").show(ui, |ui| {
            ui.label(egui::RichText::new("JSON merged over the chart's ECharts option, for what the controls cannot do. It changes the HTML page only: the chart in the app and the Excel chart ignore it.").size(theme::TEXT_LABEL).color(tk.muted));
            let r = ui.add(egui::TextEdit::multiline(&mut spec.advanced_json).font(egui::TextStyle::Monospace).desired_rows(6).desired_width(f32::INFINITY).lock_focus(true).hint_text("{ \"legend\": { \"show\": false } }"));
            rects.fields.push(("Advanced options".to_string(), r.rect));
            match micro_parquet::charts::echarts::validate_advanced(&spec.advanced_json) {
                Ok(_) => {}
                Err(e) => {
                    ui.add(egui::Label::new(egui::RichText::new(format!("{:#}", e)).size(theme::TEXT_LABEL).color(tk.danger)).wrap());
                }
            }
            ui.horizontal(|ui| {
                let b = ui.add_enabled(!spec.advanced_json.is_empty(), egui::Button::new("Reset"));
                rects.fields.push(("Advanced reset".to_string(), b.rect));
                if b.clicked() {
                    spec.advanced_json.clear();
                }
            });
            ui.label(egui::RichText::new(format!("Every option is listed in the ECharts option reference: {}", micro_parquet::charts::echarts::ECHARTS_OPTION_REFERENCE)).size(theme::TEXT_LABEL).color(tk.muted));
        });
        rects.fields.push(("Advanced options header".to_string(), h.header_response.rect));
    }

    /// The buttons of the studio: save, open and the two kinds of tab.
    pub(crate) fn studio_card_ids() -> Vec<&'static str> {
        micro_parquet::charts::gallery::CARDS.iter().map(|c| c.id).collect()
    }
}

/// A card by its id (tests).
pub fn card(id: &str) -> Option<&'static Card> {
    card_by_id(id)
}
