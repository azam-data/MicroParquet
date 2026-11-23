//! The command list in the UI (Phase 3 T6): running a command by id, icons, and the Ctrl+K
//! search overlay. The toolbar and the menus draw their items from the same list.

use crate::ui::prelude::*;
use micro_parquet::commands::{self as cmds, CommandId};

/// The icon of a command (an icon-font glyph).
pub fn command_icon(id: CommandId) -> &'static str {
    use theme::ph;
    match id {
        CommandId::OpenFile => ph::FOLDER_OPEN,
        CommandId::DirectConvert => ph::LIGHTNING,
        CommandId::Batch => ph::ARROWS_CLOCKWISE,
        CommandId::Stack => ph::STACK,
        CommandId::SaveMacro => ph::FLOPPY_DISK,
        CommandId::LoadMacro | CommandId::LoadMacroNewTab => ph::PLAY,
        CommandId::PythonExport => ph::FILE_PY,
        CommandId::Quit => ph::X,
        CommandId::Undo | CommandId::Redo => ph::ARROWS_LEFT_RIGHT,
        CommandId::Filter => ph::FUNNEL,
        CommandId::Sort => ph::SORT_ASCENDING,
        CommandId::Columns => ph::COLUMNS,
        CommandId::CalcColumn => ph::CALCULATOR,
        CommandId::Pivot => ph::TABLE,
        CommandId::Conditional => ph::PLUS,
        CommandId::Normaliser => ph::BUILDINGS,
        CommandId::CleanBreaks => ph::BROOM,
        CommandId::SplitDate => ph::CALENDAR,
        CommandId::JoinTabs => ph::LINK,
        CommandId::CopyTable => ph::CLIPBOARD_TEXT,
        CommandId::Buckets => ph::CALENDAR,
        CommandId::GapLadder => ph::CHART_BAR,
        CommandId::IrrbbLadder => ph::CHART_BAR,
        CommandId::IrrbbSix => ph::CHART_LINE,
        CommandId::BehaviourProfile => ph::CALCULATOR,
        CommandId::RateProfile => ph::PERCENT,
        CommandId::RateShock => ph::CHART_LINE,
        CommandId::Duration => ph::TIMER,
        CommandId::Ftp => ph::CALCULATOR,
        CommandId::YieldCurves => ph::CHART_LINE,
        CommandId::Provision => ph::BANK,
        CommandId::ClassifyOverdue => ph::CALENDAR,
        CommandId::Cl1Summary => ph::TABLE,
        CommandId::IfrsEcl => ph::CHART_LINE,
        CommandId::Liquidity => ph::CHART_BAR,
        CommandId::ExportCl1 => ph::TABLE,
        CommandId::Limits => ph::CREDIT_CARD,
        CommandId::Reconcile => ph::SCALES,
        CommandId::ClMigration => ph::ARROWS_LEFT_RIGHT,
        CommandId::Audit => ph::SHIELD_CHECK,
        CommandId::ExportParquet | CommandId::ExportCsv => ph::EXPORT,
        CommandId::ExportXlsx => ph::FILE_XLS,
        CommandId::ExportPdf => ph::FILE_PDF,
        CommandId::ExportSplit => ph::FILES,
        CommandId::ParquetArchive | CommandId::ParquetSnappy => ph::GEAR,
        CommandId::ToggleTheme => ph::MOON,
        CommandId::ZoomIn | CommandId::ZoomOut | CommandId::ZoomReset | CommandId::ZoomSetDefault => ph::MAGNIFYING_GLASS,
        CommandId::ScaleCrore
        | CommandId::ScaleLakh
        | CommandId::ScaleMillion
        | CommandId::ScaleThousand
        | CommandId::ScalePlain
        | CommandId::ScaleRaw => ph::HASH,
        CommandId::DecimalsUp | CommandId::DecimalsDown => ph::PERCENT,
        CommandId::TogglePanel => ph::SIDEBAR_SIMPLE,
        CommandId::ToggleSql => ph::TERMINAL_WINDOW,
        CommandId::ToggleChart => ph::CHART_BAR,
        CommandId::ChartNew | CommandId::ChartThis => ph::CHART_BAR,
        CommandId::ChartOpenSaved => ph::FOLDER_OPEN,
        CommandId::ChartBook => ph::FILES,
        CommandId::ExportChartHtml => ph::FILE_HTML,
        CommandId::ExportChartXlsx => ph::FILE_XLS,
        CommandId::ToggleSummary => ph::SIGMA,
        CommandId::SortLists => ph::SORT_ASCENDING,
        CommandId::CalendarSettings => ph::CALENDAR,
        CommandId::SavedRuns => ph::CLOCK_COUNTER_CLOCKWISE,
        CommandId::ProjectNew => ph::FILE_PLUS,
        CommandId::ProjectOpen => ph::FOLDER_OPEN,
        CommandId::ProjectRecent => ph::CLOCK_COUNTER_CLOCKWISE,
        CommandId::ProjectSave | CommandId::ProjectSaveAs => ph::FLOPPY_DISK,
        CommandId::ProjectClose => ph::X_CIRCLE,
        CommandId::Packs => ph::TABLE,
        CommandId::Mudaraba => ph::SCALES,
        CommandId::DqScorecard => ph::SCALES,
        CommandId::MomVariance => ph::CHART_LINE,
        CommandId::About => ph::INFO,
    }
}

/// The short toolbar text of a command.
pub fn toolbar_label(id: CommandId) -> &'static str {
    match id {
        CommandId::OpenFile => "Open",
        CommandId::Stack => "Stack",
        CommandId::Batch => "Batch",
        CommandId::Filter => "Filter",
        CommandId::Sort => "Sort",
        CommandId::Columns => "Columns",
        CommandId::CalcColumn => "Calc column",
        CommandId::Pivot => "Pivot",
        CommandId::RateProfile => "Rate profile",
        CommandId::ToggleChart => "Chart",
        CommandId::ToggleSql => "SQL",
        other => cmds::command(other).map(|c| c.title).unwrap_or(""),
    }
}

/// One line of the search results.
struct Item {
    /// A command, or `None` for a column of the active tab.
    id: Option<CommandId>,
    column: Option<String>,
    label: String,
    words: String,
    group: &'static str,
    reason: Option<&'static str>,
    /// The whole reason, when the short one is not all of it (a chart tab names its source tab): the hover and the status line.
    full_reason: Option<String>,
    recent: Option<usize>,
}

impl MicroParquetApp {
    pub(crate) fn command_state(&self) -> cmds::CommandState {
        let tab = self.tabs.get(self.active_tab);
        cmds::CommandState {
            tabs: self.tabs.len(),
            result_tab: tab.is_some_and(|t| is_result_source(&t.source) && !self.chart_tabs.contains_key(&t.id)),
            can_undo: tab.is_some_and(|t| t.history.can_undo()),
            can_redo: tab.is_some_and(|t| t.history.can_redo()),
            project: self.project.active,
            chart_tab: self.active_is_chart_tab(),
        }
    }

    /// The whole text of a short reason: on a chart tab the note of the steps panel, which names the source tab.
    fn full_reason_of(&self, short: &'static str) -> String {
        if short == cmds::CHART_TAB_STEP_REASON {
            if let Some(note) = self.chart_tab_steps_note() {
                return note;
            }
        }
        short.to_string()
    }

    /// Why a command cannot run now (shown greyed in the search and the menus).
    pub(crate) fn command_unavailable(&self, id: CommandId) -> Option<String> {
        // a tab of a project that did not open has no rows: nothing to export, copy or script
        if self.active_is_unavailable()
            && matches!(
                id,
                CommandId::ExportParquet | CommandId::ExportCsv | CommandId::ExportXlsx | CommandId::ExportCl1 | CommandId::ExportPdf | CommandId::ExportSplit | CommandId::CopyTable | CommandId::PythonExport | CommandId::SaveMacro
            )
        {
            return Some("This tab could not be opened, so it has nothing to export. Its reason is in the main area.".to_string());
        }
        cmds::command(id).and_then(|c| cmds::unavailable_reason(c, self.command_state())).map(|r| self.full_reason_of(r))
    }

    /// A menu or toolbar item for a command: disabled with the reason when it cannot run.
    pub(crate) fn command_menu_item(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, id: CommandId) {
        let Some(c) = cmds::command(id) else {
            return;
        };
        self.command_menu_item_titled(ui, ctx, id, c.title);
    }

    /// A menu item that runs a command under a shorter name than its title in the Ctrl+K search.
    pub(crate) fn command_menu_item_titled(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, id: CommandId, title: &str) {
        let reason = self.command_unavailable(id);
        let resp = ui.add_enabled(reason.is_none(), egui::Button::new(format!("{}  {}", command_icon(id), title)));
        let resp = match reason {
            Some(r) => resp.on_disabled_hover_text(r),
            None => resp,
        };
        if resp.clicked() {
            ui.close_menu();
            self.run_command(ctx, id);
        }
    }

    /// Runs a command and remembers it as recently used.
    pub(crate) fn run_command(&mut self, ctx: &egui::Context, id: CommandId) {
        if let Some(why) = self.command_unavailable(id) {
            self.status_message = why.to_string();
            return;
        }
        cmds::note_used(&mut self.recent_commands, id, 12);
        let _ = std::fs::write(self.paths.root_dir.join("recent_commands.txt"), cmds::recent_to_text(&self.recent_commands));
        match id {
            CommandId::OpenFile => self.initiate_file_open(false),
            CommandId::DirectConvert => self.initiate_file_open(true),
            CommandId::Batch => self.batch_modal.open = true,
            CommandId::Stack => self.active_tool = ActiveToolModal::StackFiles,
            CommandId::SaveMacro => self.active_tool = ActiveToolModal::SaveMacro,
            CommandId::LoadMacro => self.load_and_apply_macro_interactive(false),
            CommandId::LoadMacroNewTab => self.load_and_apply_macro_interactive(true),
            CommandId::PythonExport => self.prepare_python_preview_for_active_tab(),
            CommandId::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            CommandId::ProjectNew | CommandId::ProjectOpen | CommandId::ProjectRecent | CommandId::ProjectSave | CommandId::ProjectSaveAs | CommandId::ProjectClose => self.run_project_command(id),
            CommandId::Undo => self.undo_steps(),
            CommandId::Redo => self.redo_steps(),
            CommandId::Filter => self.open_filter_editor_default(),
            CommandId::Sort => self.open_sort_editor(None),
            CommandId::Columns => {
                self.init_tool_defaults_from_active_tab();
                self.active_tool = ActiveToolModal::ManageColumns;
            }
            CommandId::CalcColumn => self.active_tool = ActiveToolModal::MathFormula,
            CommandId::Pivot => {
                self.init_tool_defaults_from_active_tab();
                self.open_pivot_builder(None, false);
                self.active_tool = ActiveToolModal::PivotBuilder;
            }
            CommandId::Conditional => self.active_tool = ActiveToolModal::ConditionalCol,
            CommandId::Normaliser => self.active_tool = ActiveToolModal::NormalizeEntity,
            CommandId::SplitDate => {
                let inspected = self.selected_column.clone().filter(|c| {
                    self.tabs.get(self.active_tab).and_then(|t| t.cached_slice.column(c).ok()).is_some_and(|s| micro_parquet::dates::is_date_type(s.dtype()))
                });
                self.open_split_date(inspected);
            }
            CommandId::CleanBreaks => {
                self.clean_breaks = CleanBreaksState::default();
                self.active_tool = ActiveToolModal::CleanLineBreaks;
            }
            CommandId::JoinTabs => self.active_tool = ActiveToolModal::JoinTabs,
            CommandId::CopyTable => self.copy_table_to_clipboard(ctx),
            CommandId::GapLadder => self.open_gap_ladder(),
            CommandId::IrrbbLadder => self.open_cf_ladder(),
            CommandId::IrrbbSix => self.open_irrbb(),
            CommandId::BehaviourProfile => self.open_behaviour_profile(),
            CommandId::Buckets => {
                self.buckets.presets = CustomBucketDef::load_all_presets(&self.paths.buckets_file);
                self.active_tool = ActiveToolModal::MaturityBucket;
            }
            CommandId::RateProfile => {
                self.rate_band.presets = RateBandDef::load_all_presets(&self.paths.rate_bands_file());
                self.active_tool = ActiveToolModal::RateBand;
            }
            CommandId::RateShock => {
                self.shock.reload_beta_files();
                self.active_tool = ActiveToolModal::RateShockSim;
            }
            CommandId::Duration => self.active_tool = ActiveToolModal::DurationAnalysis,
            CommandId::Ftp => {
                self.ftp.reload_store();
                self.ftp.choose_default_source();
                self.active_tool = ActiveToolModal::FtpCurveMap;
            }
            CommandId::YieldCurves => self.open_yield_curves(),
            CommandId::Provision => self.active_tool = ActiveToolModal::LoanProvision,
            CommandId::ClassifyOverdue => self.open_classify_overdue(),
            CommandId::Cl1Summary => self.open_cl1_summary(),
            CommandId::IfrsEcl => self.open_ifrs9(),
            CommandId::Liquidity => self.open_liquidity(),
            CommandId::ExportCl1 => self.export_cl1_excel(),
            CommandId::Limits => self.active_tool = ActiveToolModal::LoanLimitUtil,
            CommandId::Reconcile => self.active_tool = ActiveToolModal::ReconcilePeriods,
            CommandId::ClMigration => self.open_cl_migration(),
            CommandId::Audit => self.active_tool = ActiveToolModal::BankingAudit,
            CommandId::ExportParquet => self.run_guarded_export(GuardedExport::Parquet),
            CommandId::ExportCsv => self.run_guarded_export(GuardedExport::Csv),
            CommandId::ExportXlsx | CommandId::ExportPdf => {
                self.export.multi_checks = vec![true; self.tabs.len()];
                self.active_tool = ActiveToolModal::MultiTabExport;
            }
            CommandId::ExportSplit => {
                self.init_tool_defaults_from_active_tab();
                self.active_tool = ActiveToolModal::SplitByColumn;
            }
            CommandId::ParquetArchive => self.parquet_dialect = ParquetDialect::ArchiveZstd,
            CommandId::ParquetSnappy => self.parquet_dialect = ParquetDialect::PowerBiSnappy,
            CommandId::ToggleTheme => {
                self.dark_mode = !self.dark_mode;
                theme::apply_ui_theme(ctx, self.dark_mode, self.ui_zoom);
            }
            CommandId::ZoomIn => self.zoom_in(ctx),
            CommandId::ZoomOut => self.zoom_out(ctx),
            CommandId::ZoomReset => self.zoom_reset(ctx),
            CommandId::ZoomSetDefault => {
                let z = self.ui_zoom;
                self.zoom_set_default(z);
            }
            CommandId::ScaleCrore => self.num_format = NumberDisplayFormat::BdtCrore,
            CommandId::ScaleLakh => self.num_format = NumberDisplayFormat::BdtLakh,
            CommandId::ScaleMillion => self.num_format = NumberDisplayFormat::BdtMillion,
            CommandId::ScaleThousand => self.num_format = NumberDisplayFormat::BdtThousand,
            CommandId::ScalePlain => self.num_format = NumberDisplayFormat::StandardCommas,
            CommandId::ScaleRaw => self.num_format = NumberDisplayFormat::Raw,
            CommandId::DecimalsUp => self.global_decimals = (self.global_decimals + 1).min(6),
            CommandId::DecimalsDown => self.global_decimals = self.global_decimals.saturating_sub(1),
            CommandId::TogglePanel => self.show_steps_sidebar = !self.show_steps_sidebar,
            CommandId::ToggleSql => {
                // the SQL tab of the right panel (Phase 6 U2); switched on again if View ▸ Side panel had it off
                self.set_side_tab_visible(crate::ui::steps::RightTab::Sql, true);
            }
            CommandId::ToggleChart => {
                // the Chart tab of the right panel (Phase 6 F2): shown, or hidden when it is the tab in view
                if self.side_panel_shown() && self.right_tab == crate::ui::steps::RightTab::Chart {
                    self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, false);
                } else {
                    self.set_side_tab_visible(crate::ui::steps::RightTab::Chart, true);
                }
            }
            CommandId::ToggleSummary => self.show_summary_row = !self.show_summary_row,
            CommandId::SortLists => self.open_sort_lists(),
            CommandId::CalendarSettings => self.open_calendar_settings(),
            CommandId::ChartNew => self.open_chart_gallery(),
            CommandId::ChartThis => self.chart_this_result(),
            CommandId::ChartOpenSaved => self.open_chart_open(),
            CommandId::ChartBook => self.open_chart_book(),
            CommandId::ExportChartHtml => self.chart_export_html(),
            CommandId::ExportChartXlsx => self.chart_export_xlsx(),
            CommandId::SavedRuns => self.open_saved_runs(),
            CommandId::Packs => self.open_packs(),
            CommandId::Mudaraba => self.open_mudaraba(),
            CommandId::DqScorecard => self.open_dq(),
            CommandId::MomVariance => self.open_mom(),
            CommandId::About => self.show_about = true,
        }
    }

    /// Opens the search overlay with an empty query.
    pub(crate) fn open_command_palette(&mut self) {
        self.command_open = true;
        self.command_query.clear();
        self.command_cursor = 0;
        self.command_focus = true;
        // (a click that opens it must not count as a click outside in the same frame)
        self.command_opened_pass = u64::MAX;
    }

    /// Ctrl+K opens or closes the search.
    pub(crate) fn handle_palette_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            if self.command_open {
                self.command_open = false;
            } else {
                self.open_command_palette();
            }
        }
    }

    fn palette_items(&self) -> Vec<Item> {
        let state = self.command_state();
        let mut items: Vec<Item> = cmds::COMMANDS
            .iter()
            .map(|c| Item {
                id: Some(c.id),
                column: None,
                label: c.title.to_string(),
                words: format!("{} {}", c.words, c.group.label()),
                group: c.group.label(),
                reason: cmds::unavailable_reason(c, state),
                full_reason: cmds::unavailable_reason(c, state).map(|r| self.full_reason_of(r)),
                recent: self.recent_commands.iter().position(|r| *r == c.id),
            })
            .collect();
        if let Some(tab) = self.tabs.get(self.active_tab) {
            for name in tab.available_columns() {
                items.push(Item {
                    id: None,
                    label: format!("Column: {}", name),
                    words: "column field inspector statistics".to_string(),
                    group: "Column",
                    reason: None,
                    full_reason: None,
                    recent: None,
                    column: Some(name),
                });
            }
        }
        items
    }

    /// The Ctrl+K search: type to find an action or a column, Up / Down to move, Enter to run,
    /// Esc to close. Actions that cannot run now are listed greyed with the reason.
    pub(crate) fn render_command_palette(&mut self, ctx: &egui::Context) {
        if !self.command_open {
            return;
        }
        let tk = *theme::tokens(self.dark_mode);
        let items = self.palette_items();
        let entries: Vec<cmds::Entry> = items
            .iter()
            .map(|i| cmds::Entry { label: &i.label, words: &i.words, recent: i.recent })
            .collect();
        let mut results: Vec<usize> = cmds::rank(&self.command_query, &entries);
        // With nothing typed: the recently used first, then the actions in menu order.
        results.truncate(if self.command_query.trim().is_empty() { 40 } else { 60 });

        let (down, up, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if down && !results.is_empty() {
            self.command_cursor = (self.command_cursor + 1).min(results.len() - 1);
        }
        if up {
            self.command_cursor = self.command_cursor.saturating_sub(1);
        }
        self.command_cursor = self.command_cursor.min(results.len().saturating_sub(1));

        let mut run: Option<usize> = None;
        if enter {
            run = results.get(self.command_cursor).copied();
        }
        let mut close = esc;
        // the frame it opened in: a click that was meant to open it is not "outside"
        let pass = ctx.cumulative_pass_nr();
        if self.command_opened_pass == u64::MAX {
            self.command_opened_pass = pass;
        }
        let area = egui::Area::new(egui::Id::new("command_palette"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, [0.0, theme::PALETTE_TOP])
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(tk.surface)
                    .stroke(theme::line(tk.border))
                    .rounding(egui::Rounding::same(theme::RADIUS_PALETTE))
                    .shadow(theme::palette_shadow())
                    .inner_margin(egui::Margin::same(theme::SPACE_M))
                    .show(ui, |ui| {
                        ui.set_width(theme::PALETTE_W);
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.command_query)
                                .hint_text("Search actions and columns")
                                .desired_width(f32::INFINITY),
                        );
                        if self.command_focus {
                            edit.request_focus();
                            self.command_focus = false;
                        }
                        if edit.changed() {
                            self.command_cursor = 0;
                        }
                        ui.add_space(theme::SPACE_S);
                        egui::ScrollArea::vertical().max_height(theme::PALETTE_LIST_MAX_H).show(ui, |ui| {
                            if results.is_empty() {
                                ui.label(egui::RichText::new("Nothing matches.").color(tk.muted));
                            }
                            for (row, idx) in results.iter().enumerate() {
                                let it = &items[*idx];
                                let on_cursor = row == self.command_cursor;
                                let fill = if on_cursor { tk.accent_soft } else { egui::Color32::TRANSPARENT };
                                let color = if it.reason.is_some() { tk.muted } else { tk.ink };
                                let r = egui::Frame::none()
                                    .fill(fill)
                                    .rounding(egui::Rounding::same(theme::RADIUS_CONTROL))
                                    .inner_margin(theme::MARGIN_PALETTE_ROW)
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                if let Some(id) = it.id {
                                                    ui.label(egui::RichText::new(command_icon(id)).color(tk.ink2));
                                                } else {
                                                    ui.label(egui::RichText::new(theme::ph::TABLE).color(tk.ink2));
                                                }
                                                ui.label(egui::RichText::new(&it.label).color(color));
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.label(theme::label_text(it.group).color(tk.muted));
                                                });
                                            });
                                            // the reason has a line of its own and wraps: a long one never runs into the title
                                            if let Some(why) = it.reason {
                                                ui.add(egui::Label::new(theme::label_text(why).color(tk.muted)).wrap());
                                            }
                                        });
                                    })
                                    .response
                                    .interact(egui::Sense::click());
                                // the whole reason on hover, as on the greyed menu items
                                let r = match &it.full_reason {
                                    Some(full) => r.on_hover_text(full),
                                    None => r,
                                };
                                if on_cursor && (down || up) {
                                    r.scroll_to_me(None);
                                }
                                if r.clicked() {
                                    run = Some(*idx);
                                }
                            }
                        });
                    });
            });
        // a click anywhere else (the search box in the bar counts as inside: it toggles), or the
        // window losing focus, closes it
        let (pressed_at, focused) = ctx.input(|i| (if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None }, i.focused));
        let outside = pressed_at.is_some_and(|p| !area.response.rect.contains(p) && !self.command_box_rect.contains(p));
        if (outside && self.command_opened_pass != pass) || !focused {
            close = true;
        }

        if let Some(idx) = run {
            let it = &items[idx];
            if let Some(why) = it.reason {
                self.status_message = it.full_reason.clone().unwrap_or_else(|| why.to_string());
            } else {
                close = true;
                match (&it.id, &it.column) {
                    (Some(id), _) => {
                        let id = *id;
                        self.command_open = false;
                        self.run_command(ctx, id);
                    }
                    (None, Some(col)) => {
                        let col = col.clone();
                        self.select_column(&col);
                    }
                    _ => {}
                }
            }
        }
        if close {
            self.command_open = false;
        }
    }
}

/// True for a tab a tool made (gap ladder, IRRBB, MoM variance, …) rather than a file, a stack or a join.
pub fn is_result_source(s: &TabSource) -> bool {
    !matches!(s, TabSource::SingleFile { .. } | TabSource::VerticalStack { .. } | TabSource::JoinedTabs { .. })
}
