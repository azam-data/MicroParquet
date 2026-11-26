//! The workspace (UI revamp, Phase 10): the three views of one open project (Report, Data, Model), which commands belong
//! to which workflow, what each view's toolbar offers, how far the model's set-up has got, the relationship diagram and
//! the hints an empty screen gives. All of it is plain logic (no drawing), so it is tested without a window.

use crate::commands::{self as cmds, CommandGroup, CommandId};

// ------------------------------------------------------------------------------------------------- the views

/// The three views of the window, switched by the rail on the left. They are views of one project: the pivots, charts and
/// dashboards all read the same loaded data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum View {
    Report,
    Dashboard,
    #[default]
    Data,
    Model,
}

impl View {
    pub const ALL: [View; 4] = [View::Report, View::Dashboard, View::Data, View::Model];

    /// The word on the rail and in `settings.json`.
    pub fn label(self) -> &'static str {
        match self {
            View::Report => "Report",
            View::Dashboard => "Dashboard",
            View::Data => "Data",
            View::Model => "Model",
        }
    }

    /// What the view is for, in one line (the rail's hover text).
    pub fn purpose(self) -> &'static str {
        match self {
            View::Report => "Pivots, charts and saved views",
            View::Dashboard => "Pages of charts, tables, KPI cards, text and pictures, shown as one board and exported to HTML, PowerPoint and PDF",
            View::Data => "Source files, steps, SQL, column details and single-table banking tools",
            View::Model => "Relationships, key check, calendar, as-of date and measures",
        }
    }

    /// The view named in `settings.json`; an unknown name is `None` (the default view is used).
    pub fn from_name(name: &str) -> Option<View> {
        View::ALL.iter().copied().find(|v| v.label().eq_ignore_ascii_case(name.trim()))
    }
}

/// Which view's tab strip a tab is listed in. Data lists every table tab (a pivot is a table with a pivot step, and its steps
/// are edited there); Report lists the chart tabs, the pivot tabs and the tabs sent to the report. A chart tab has no data of its own.
pub fn tab_in_view(view: View, chart_tab: bool, pivot_tab: bool, sent_to_report: bool) -> bool {
    match view {
        View::Data => !chart_tab,
        View::Report => chart_tab || pivot_tab || sent_to_report,
        View::Model | View::Dashboard => false,
    }
}

/// The view a command works in. Running it from Ctrl+K switches there first. `None`: it works wherever you are (or it is
/// about the project and the app, not a table).
pub fn home_view(id: CommandId) -> Option<View> {
    use CommandId as I;
    match id {
        I::Dashboard => Some(View::Dashboard),
        I::ModelSetup | I::ModelLinks | I::ModelKeyCheck | I::ModelCalendar | I::ModelAsOf | I::ModelMeasures => Some(View::Model),
        _ if cmds::adds_step_to_active_tab(id) && id != I::Pivot => Some(View::Data),
        I::OpenFile
        | I::DirectConvert
        | I::Batch
        | I::Stack
        | I::SaveMacro
        | I::LoadMacro
        | I::LoadMacroNewTab
        | I::Conditional
        | I::Normaliser
        | I::CleanBreaks
        | I::JoinTabs
        | I::CopyTable
        | I::GapLadder
        | I::IrrbbLadder
        | I::IrrbbSix
        | I::Liquidity
        | I::BehaviourProfile
        | I::RateShock
        | I::YieldCurves
        | I::Cl1Summary
        | I::Reconcile
        | I::ClMigration
        | I::Audit
        | I::DqScorecard
        | I::MomVariance
        | I::TopMovers
        | I::Mudaraba
        | I::Packs
        | I::SavedRuns
        | I::ToggleSql => Some(View::Data),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------- workflow names

/// The workflow a command is listed under in the search (the small tag at the right of each result).
pub fn workflow_of(id: CommandId) -> &'static str {
    use CommandId as I;
    let Some(c) = cmds::command(id) else { return "" };
    match id {
        I::OpenFile | I::DirectConvert | I::Batch | I::Stack | I::SaveMacro | I::LoadMacro | I::LoadMacroNewTab | I::Undo | I::Redo | I::ToggleSql => "Data",
        I::PythonExport => "Share",
        I::SavedRuns | I::Pivot | I::Audit | I::DqScorecard => "Analyse",
        I::ToggleChart => "Visualise",
        I::Settings | I::TogglePanel => "Settings",
        I::GoReport | I::GoDashboard | I::GoData | I::GoModel | I::SendToReport | I::ToggleAnalytics => "Workspace",
        _ => match c.group {
            CommandGroup::File => "File",
            CommandGroup::Edit | CommandGroup::Transform => "Data",
            CommandGroup::Banking => "Analyse",
            CommandGroup::Charts => "Visualise",
            CommandGroup::Export => "Share",
            CommandGroup::View => "Settings",
            CommandGroup::Help => "Help",
            CommandGroup::Model => "Model",
            CommandGroup::Workspace => "Workspace",
        },
    }
}

// ------------------------------------------------------------------------------------------------- the bar

/// The menus under the view's toolbar buttons.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MenuKind {
    /// Data view: clean and reshape the table.
    Prepare,
    /// Data view: the single-table banking tools and the data checks.
    Analyse,
    /// Data view: write the table out.
    ShareData,
    /// Report view: charts.
    Visualise,
    /// Report view: write the charts, pivots and pages out.
    ShareReport,
}

impl MenuKind {
    pub fn label(self) -> &'static str {
        match self {
            MenuKind::Prepare => "Prepare",
            MenuKind::Analyse => "Analyse",
            MenuKind::ShareData | MenuKind::ShareReport => "Share",
            MenuKind::Visualise => "Visualise",
        }
    }
}

/// One line of a menu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MenuEntry {
    Cmd(CommandId),
    /// A sub-menu: its name and the commands in it.
    Sub(&'static str, Vec<CommandId>),
    Separator,
}

/// What a menu lists, in order.
pub fn menu_entries(kind: MenuKind) -> Vec<MenuEntry> {
    use CommandId as I;
    use MenuEntry::{Cmd, Separator, Sub};
    match kind {
        MenuKind::Prepare => vec![
            Cmd(I::Conditional),
            Cmd(I::CleanBreaks),
            Cmd(I::SplitDate),
            Cmd(I::Normaliser),
            Cmd(I::JoinTabs),
            Separator,
            Cmd(I::Undo),
            Cmd(I::Redo),
            Cmd(I::CopyTable),
            Separator,
            Cmd(I::SaveMacro),
            Cmd(I::LoadMacro),
            Cmd(I::LoadMacroNewTab),
        ],
        MenuKind::Analyse => {
            let mut sections: Vec<(&'static str, Vec<CommandId>)> = Vec::new();
            for c in cmds::in_group(CommandGroup::Banking) {
                let name = cmds::banking_section(c.id).unwrap_or("Other");
                match sections.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, v)) => v.push(c.id),
                    None => sections.push((name, vec![c.id])),
                }
            }
            let mut v: Vec<MenuEntry> = sections.into_iter().map(|(n, ids)| Sub(n, ids)).collect();
            v.push(Separator);
            v.push(Sub("Data checks", cmds::DATA_CHECKS.to_vec()));
            v.push(Cmd(I::SavedRuns));
            v
        }
        MenuKind::ShareData => vec![
            Cmd(I::ExportParquet),
            Cmd(I::ExportCsv),
            Cmd(I::ExportXlsx),
            Cmd(I::ExportCl1),
            Cmd(I::ExportPdf),
            Cmd(I::ExportSplit),
            Cmd(I::PythonExport),
            Separator,
            Cmd(I::ParquetArchive),
            Cmd(I::ParquetSnappy),
        ],
        MenuKind::Visualise => vec![Cmd(I::ChartNew), Cmd(I::ChartThis), Cmd(I::ChartOpenSaved), Cmd(I::ChartBook)],
        MenuKind::ShareReport => vec![Cmd(I::ExportChartHtml), Cmd(I::ExportChartXlsx), Cmd(I::ExportXlsx), Cmd(I::ExportPdf), Separator, Cmd(I::ExportTabPptx), Cmd(I::Dashboard)],
    }
}

/// An item of a view's toolbar: a button for a command, or a menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BarItem {
    Cmd(CommandId),
    Menu(MenuKind),
}

/// A toolbar item and the group it is drawn in (groups are separated by a thin line).
pub type BarEntry = (BarItem, usize);

/// The toolbar of a view, in drawing order. Buttons fold from the right into "»" when the window is narrow.
pub fn view_bar(view: View) -> Vec<BarEntry> {
    use BarItem::{Cmd, Menu};
    use CommandId as I;
    match view {
        View::Data => {
            // the menus come right after the file buttons, so a narrow window folds the buttons that have a shortcut elsewhere first
            let mut v: Vec<BarEntry> = crate::commands::TOOLBAR_BUTTONS_FILE.iter().map(|id| (Cmd(*id), 0)).collect();
            v.push((Menu(MenuKind::Prepare), 1));
            v.push((Menu(MenuKind::Analyse), 1));
            v.push((Menu(MenuKind::ShareData), 1));
            v.extend(crate::commands::TOOLBAR_BUTTONS_TRANSFORM.iter().map(|id| (Cmd(*id), 2)));
            v.push((Cmd(I::ToggleChart), 3));
            v.push((Cmd(I::ToggleSql), 3));
            v.push((Cmd(I::SendToReport), 3));
            v
        }
        View::Report => vec![
            (Menu(MenuKind::Visualise), 0),
            (Cmd(I::ToggleChart), 0),
            (Cmd(I::Pivot), 1),
            (Menu(MenuKind::ShareReport), 2),
            (Cmd(I::ToggleAnalytics), 3),
        ],
        View::Dashboard => vec![],
        View::Model => vec![(Cmd(I::ModelSetup), 0), (Cmd(I::ModelLinks), 1), (Cmd(I::ModelKeyCheck), 1), (Cmd(I::ModelCalendar), 1), (Cmd(I::ModelAsOf), 1), (Cmd(I::ModelMeasures), 1)],
    }
}

/// The items of the Settings menu (the menu also holds the zoom, toolbar and side-panel choices, which are not commands).
pub const SETTINGS_MENU: &[CommandId] = &[CommandId::Settings, CommandId::ToggleTheme];

/// Commands that have no button or menu entry: they are found with Ctrl+K and set in the Settings dialog (Units & numbers, Display).
pub const SETTINGS_ONLY: &[CommandId] = &[
    CommandId::ScaleCrore,
    CommandId::ScaleLakh,
    CommandId::ScaleMillion,
    CommandId::ScaleThousand,
    CommandId::ScalePlain,
    CommandId::ScaleRaw,
    CommandId::DecimalsUp,
    CommandId::DecimalsDown,
    CommandId::ZoomIn,
    CommandId::ZoomOut,
    CommandId::ZoomReset,
    CommandId::ZoomSetDefault,
    CommandId::TogglePanel,
    CommandId::ToggleSummary,
    CommandId::SortLists,
    CommandId::CalendarSettings,
    CommandId::HouseColours,
];

/// Every command a person can reach without Ctrl+K: the File, Settings and Help menus, each view's toolbar and its menus.
pub fn reachable_commands() -> Vec<CommandId> {
    let mut v: Vec<CommandId> = Vec::new();
    v.extend(cmds::in_group(CommandGroup::File).map(|c| c.id));
    v.extend_from_slice(SETTINGS_MENU);
    v.extend(cmds::in_group(CommandGroup::Help).map(|c| c.id));
    v.extend_from_slice(&[CommandId::GoReport, CommandId::GoDashboard, CommandId::GoData, CommandId::GoModel]);
    for view in View::ALL {
        for (item, _) in view_bar(view) {
            match item {
                BarItem::Cmd(id) => v.push(id),
                BarItem::Menu(m) => {
                    for e in menu_entries(m) {
                        match e {
                            MenuEntry::Cmd(id) => v.push(id),
                            MenuEntry::Sub(_, ids) => v.extend(ids),
                            MenuEntry::Separator => {}
                        }
                    }
                }
            }
        }
    }
    v
}

// ------------------------------------------------------------------------------------------------- settings

/// The list on the left of the Settings dialog.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum SettingsSection {
    Display,
    #[default]
    Units,
    Calendar,
    AsOf,
    Colours,
    SortLists,
    Toolbar,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 7] = [
        SettingsSection::Display,
        SettingsSection::Units,
        SettingsSection::Calendar,
        SettingsSection::AsOf,
        SettingsSection::Colours,
        SettingsSection::SortLists,
        SettingsSection::Toolbar,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::Display => "Display",
            SettingsSection::Units => "Units & numbers",
            SettingsSection::Calendar => "Calendar & fiscal year",
            SettingsSection::AsOf => "As-of date",
            SettingsSection::Colours => "Colours",
            SettingsSection::SortLists => "Sort lists",
            SettingsSection::Toolbar => "Toolbar & panels",
        }
    }
}

// ------------------------------------------------------------------------------------------------- model set-up

/// The steps of the model's set-up, in the order to do them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ModelStep {
    Tables,
    Links,
    KeyCheck,
    Calendar,
    AsOf,
    Measures,
}

impl ModelStep {
    pub const ALL: [ModelStep; 6] = [ModelStep::Tables, ModelStep::Links, ModelStep::KeyCheck, ModelStep::Calendar, ModelStep::AsOf, ModelStep::Measures];

    pub fn label(self) -> &'static str {
        match self {
            ModelStep::Tables => "Tables",
            ModelStep::Links => "Links",
            ModelStep::KeyCheck => "Key check",
            ModelStep::Calendar => "Calendar",
            ModelStep::AsOf => "As-of date",
            ModelStep::Measures => "Measures",
        }
    }

    pub fn index(self) -> usize {
        ModelStep::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn next(self) -> Option<ModelStep> {
        ModelStep::ALL.get(self.index() + 1).copied()
    }

    pub fn previous(self) -> Option<ModelStep> {
        self.index().checked_sub(1).and_then(|i| ModelStep::ALL.get(i).copied())
    }
}

/// What the set-up has so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelProgress {
    /// Open tabs that hold data (not chart tabs).
    pub tables: usize,
    pub links: usize,
    /// Links whose key check has run.
    pub checked: usize,
    /// Links that cannot be used (a tab is closed, a column is missing, duplicate keys).
    pub broken: usize,
    /// Links that work but leave some data keys unmatched.
    pub unmatched: usize,
    pub calendar: bool,
    pub as_of_chosen: bool,
    pub measures: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    /// Still to do.
    Todo,
    /// Nothing is wrong, but the step is not needed to use the model.
    Optional,
    /// Done, with something to fix.
    Problem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepInfo {
    pub step: ModelStep,
    pub state: StepState,
    /// One line for the status: "2 links", "2 problems", "Latest data date".
    pub note: String,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

/// Each step's state and its one-line note.
pub fn model_steps(p: &ModelProgress) -> Vec<StepInfo> {
    let mut out = Vec::new();
    out.push(StepInfo {
        step: ModelStep::Tables,
        state: if p.tables > 0 { StepState::Done } else { StepState::Todo },
        note: if p.tables > 0 { plural(p.tables, "table open", "tables open") } else { "Open the files the model uses".to_string() },
    });
    out.push(StepInfo {
        step: ModelStep::Links,
        state: if p.links > 0 { StepState::Done } else { StepState::Todo },
        note: if p.links > 0 { plural(p.links, "link", "links") } else { "No links yet".to_string() },
    });
    let problems = p.broken + p.unmatched;
    out.push(StepInfo {
        step: ModelStep::KeyCheck,
        state: if p.links == 0 {
            StepState::Todo
        } else if problems > 0 {
            StepState::Problem
        } else if p.checked >= p.links {
            StepState::Done
        } else {
            StepState::Todo
        },
        note: if p.links == 0 {
            "Needs a link first".to_string()
        } else if problems > 0 {
            plural(problems, "problem", "problems")
        } else if p.checked >= p.links {
            "All keys match".to_string()
        } else {
            "Not run yet".to_string()
        },
    });
    out.push(StepInfo {
        step: ModelStep::Calendar,
        state: if p.calendar { StepState::Done } else { StepState::Todo },
        note: if p.calendar { "Shared calendar set".to_string() } else { "Needed for month and year comparisons".to_string() },
    });
    out.push(StepInfo {
        step: ModelStep::AsOf,
        state: if p.calendar { StepState::Done } else { StepState::Optional },
        note: if p.as_of_chosen { "Date chosen".to_string() } else { "Latest data date".to_string() },
    });
    out.push(StepInfo {
        step: ModelStep::Measures,
        state: if p.measures > 0 { StepState::Done } else { StepState::Optional },
        note: if p.measures > 0 { plural(p.measures, "measure", "measures") } else { "Optional: yield, cost of funds, NIM".to_string() },
    });
    out
}

/// The step to open when the set-up is started from the Model view: the first that is not done (a problem counts as not done), else the last.
pub fn first_open_step(steps: &[StepInfo]) -> ModelStep {
    steps.iter().find(|s| matches!(s.state, StepState::Todo | StepState::Problem)).map(|s| s.step).unwrap_or(ModelStep::Measures)
}

// ------------------------------------------------------------------------------------------------- the diagram

/// A table in the relationship diagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramNode {
    pub key: String,
    pub title: String,
    /// Column of the diagram, from the left: data tables first, lookups after.
    pub column: usize,
    pub row: usize,
}

/// A link in the diagram: data table → lookup table, and what its key check says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramEdge {
    pub from: String,
    pub to: String,
    /// "Matched", "2 duplicate keys", "Not checked"…
    pub status: String,
    pub bad: bool,
}

/// What the diagram draws.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Diagram {
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
    pub columns: usize,
    pub rows: usize,
}

/// A link as the diagram needs it: the keys and titles of the two tabs and the key check's words.
#[derive(Clone, Debug)]
pub struct DiagramLink {
    pub data_key: String,
    pub data_title: String,
    pub lookup_key: String,
    pub lookup_title: String,
    pub status: String,
    pub bad: bool,
}

/// Lays the tables out: a table that is only ever a data table is in the first column, a table some link reads as a lookup is in
/// the next column after every table that points to it. The calendar (when there is one) is a lookup too, in the last column, with
/// an edge from each table that has a date link. Order inside a column follows the order the links were made.
pub fn layout_diagram(links: &[DiagramLink], calendar: Option<(&str, &str)>, date_links: &[(String, String)]) -> Diagram {
    let mut all: Vec<DiagramLink> = links.to_vec();
    if let Some((cal_key, cal_title)) = calendar {
        for (data_key, data_title) in date_links {
            all.push(DiagramLink { data_key: data_key.clone(), data_title: data_title.clone(), lookup_key: cal_key.to_string(), lookup_title: cal_title.to_string(), status: "Date link".to_string(), bad: false });
        }
    }
    let mut order: Vec<(String, String)> = Vec::new();
    for l in &all {
        for (k, t) in [(&l.data_key, &l.data_title), (&l.lookup_key, &l.lookup_title)] {
            if !order.iter().any(|(ok, _)| ok == k) {
                order.push((k.clone(), t.clone()));
            }
        }
    }
    // a column is one more than the deepest table that points to the table (links never loop; the bound keeps a bad file from looping us)
    let mut col: Vec<usize> = vec![0; order.len()];
    let idx = |k: &str| order.iter().position(|(ok, _)| ok == k);
    for _ in 0..order.len() {
        let mut changed = false;
        for l in &all {
            if let (Some(a), Some(b)) = (idx(&l.data_key), idx(&l.lookup_key)) {
                if a != b && col[b] < col[a] + 1 {
                    col[b] = col[a] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut counts: Vec<usize> = Vec::new();
    let mut nodes = Vec::new();
    for (i, (key, title)) in order.iter().enumerate() {
        let c = col[i];
        if counts.len() <= c {
            counts.resize(c + 1, 0);
        }
        nodes.push(DiagramNode { key: key.clone(), title: title.clone(), column: c, row: counts[c] });
        counts[c] += 1;
    }
    let edges = all.iter().map(|l| DiagramEdge { from: l.data_key.clone(), to: l.lookup_key.clone(), status: l.status.clone(), bad: l.bad }).collect();
    Diagram { nodes, edges, columns: counts.len(), rows: counts.iter().copied().max().unwrap_or(0) }
}

// ------------------------------------------------------------------------------------------------- empty screens

/// What a button of an empty screen does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HintAction {
    Run(CommandId),
    Go(View),
    /// A dashboard in the project (made when there is none).
    NewDashboard,
}

/// One suggestion on an empty screen: a card with a title, a line and a button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintCard {
    pub title: &'static str,
    pub body: &'static str,
    pub button: &'static str,
    pub action: HintAction,
}

/// An empty screen: what is missing and the likely next steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub title: &'static str,
    pub line: &'static str,
    pub cards: Vec<HintCard>,
}

/// What the empty screen's text depends on.
#[derive(Clone, Copy, Debug, Default)]
pub struct HintState {
    /// Tabs of the Data view.
    pub data_tabs: usize,
    /// Tabs of the Report view.
    pub report_tabs: usize,
    pub project: bool,
    pub dashboard: bool,
    pub links: usize,
}

fn card(title: &'static str, body: &'static str, button: &'static str, action: HintAction) -> HintCard {
    HintCard { title, body, button, action }
}

/// The hint of a view that has nothing to show. `None` when the view has something (no hint then).
pub fn empty_hint(view: View, s: HintState) -> Option<Hint> {
    match view {
        View::Data if s.data_tabs == 0 => Some(Hint {
            title: "No data yet",
            line: "Open a CSV or Parquet file to start. Everything you open here can feed pivots, charts and dashboards.",
            cards: vec![
                card("Open a file", "A CSV or Parquet file from the core banking system", "Open file\u{2026}", HintAction::Run(CommandId::OpenFile)),
                card("Stack several files", "Months or branches in separate files, joined into one table", "Stack files\u{2026}", HintAction::Run(CommandId::Stack)),
                card("Convert a folder", "Many CSV files to Parquet in one go", "Batch convert\u{2026}", HintAction::Run(CommandId::Batch)),
            ],
        }),
        View::Report if s.report_tabs == 0 => Some(Hint {
            title: "Nothing in the report yet",
            line: if s.data_tabs == 0 { "Open a file in the Data view first; then a pivot, a chart or a dashboard can be made from it." } else { "Make a pivot or a chart from your data, or send a table here as it is." },
            cards: if s.data_tabs == 0 {
                vec![card("Open a file", "Start in the Data view", "Go to Data", HintAction::Go(View::Data))]
            } else {
                vec![
                    card("Pivot", "Group and total the table in the Data view", "New pivot\u{2026}", HintAction::Run(CommandId::Pivot)),
                    card("Chart", "Bars, lines, waterfall and banking templates", "New chart\u{2026}", HintAction::Run(CommandId::ChartNew)),
                    card("Send a table", "Show the table you are working on in the report", "Send to report", HintAction::Run(CommandId::SendToReport)),
                ]
            },
        }),
        View::Model if s.links == 0 => Some(Hint {
            title: "The model is empty",
            line: "Link your lookups (branch master, product master, GL map) to the data, then set the calendar and the as-of date.",
            cards: vec![
                card("Set up the model", "Tables, links, key check, calendar, as-of date and measures, one step at a time", "Start set-up\u{2026}", HintAction::Run(CommandId::ModelSetup)),
                card("Open files first", "The model links tables that are open as tabs", "Go to Data", HintAction::Go(View::Data)),
            ],
        }),
        _ => None,
    }
}

/// The empty dashboard: what to add first.
pub fn empty_dashboard_hint(chart_tabs: usize, pivot_tabs: usize) -> Hint {
    Hint {
        title: "Nothing on this dashboard yet",
        line: "Add a chart or table you already made, or start with a KPI card.",
        cards: vec![
            card("Chart tab", if chart_tabs == 0 { "No chart tab is open yet" } else { "Pick from the chart tabs that are open" }, "Add chart", HintAction::Run(CommandId::ChartNew)),
            card("Table", if pivot_tabs == 0 { "No pivot tab is open yet" } else { "Pick from the pivot tabs that are open" }, "Add table", HintAction::Run(CommandId::Pivot)),
            card("KPI card", "Control balance, NIM, ADR", "Add KPI", HintAction::NewDashboard),
        ],
    }
}

// ------------------------------------------------------------------------------------------------- the palette

/// The suggestions the search shows before anything is typed: a phrase to type and what it finds.
pub const TRY_TYPING: &[(&str, &str)] = &[("nim", "Model measures"), ("as of", "Set the as-of date"), ("crore", "Show money in Crore"), ("gap", "Liquidity gap ladder")];

/// The hint's rows for the empty search: the recent commands (those that exist), newest first, at most `n`.
pub fn recent_for_palette(recent: &[CommandId], n: usize) -> Vec<CommandId> {
    recent.iter().copied().filter(|id| cmds::command(*id).is_some()).take(n).collect()
}

// ------------------------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn views_have_names_that_round_trip() {
        for v in View::ALL {
            assert_eq!(View::from_name(v.label()), Some(v));
        }
        assert_eq!(View::from_name("data"), Some(View::Data));
        assert_eq!(View::from_name("nonsense"), None);
        assert_eq!(View::default(), View::Data);
    }

    #[test]
    fn a_tab_is_listed_where_it_belongs() {
        // (chart, pivot, sent) → Data, Report
        assert!(tab_in_view(View::Data, false, false, false) && !tab_in_view(View::Report, false, false, false));
        assert!(tab_in_view(View::Data, false, true, false) && tab_in_view(View::Report, false, true, false), "a pivot is in both");
        assert!(!tab_in_view(View::Data, true, false, false) && tab_in_view(View::Report, true, false, false), "a chart tab is in the report only");
        assert!(tab_in_view(View::Data, false, false, true) && tab_in_view(View::Report, false, false, true), "a table sent to the report is in both");
        assert!(!tab_in_view(View::Model, false, true, true));
    }

    #[test]
    fn no_command_is_lost() {
        let reach: HashSet<CommandId> = reachable_commands().into_iter().chain(SETTINGS_ONLY.iter().copied()).collect();
        for c in cmds::COMMANDS {
            assert!(reach.contains(&c.id), "{:?} ({}) is on no menu or toolbar and not in the Settings dialog", c.id, c.title);
        }
    }

    #[test]
    fn a_menu_has_at_most_eight_top_level_entries_and_no_command_twice() {
        for kind in [MenuKind::Prepare, MenuKind::Analyse, MenuKind::ShareData, MenuKind::Visualise, MenuKind::ShareReport] {
            let entries = menu_entries(kind);
            let mut seen = HashSet::new();
            let mut top = 0;
            for e in &entries {
                match e {
                    MenuEntry::Cmd(id) => {
                        top += 1;
                        assert!(seen.insert(*id), "{:?} twice in {:?}", id, kind);
                    }
                    MenuEntry::Sub(name, ids) => {
                        top += 1;
                        assert!(!ids.is_empty() && ids.len() <= 12, "{name}: {} entries", ids.len());
                        for id in ids {
                            assert!(seen.insert(*id), "{:?} twice in {:?}", id, kind);
                        }
                    }
                    MenuEntry::Separator => {}
                }
            }
            // (the Prepare and Share menus group their items with separators: about eight in a group, thirteen at most in all)
            assert!(top <= 13, "{kind:?}: {top} entries");
        }
        assert!(menu_entries(MenuKind::Analyse).iter().filter(|e| !matches!(e, MenuEntry::Separator)).count() <= 8, "the Analyse menu is short: the long banking list is in sub-menus");
    }

    #[test]
    fn the_analyse_menu_keeps_the_banking_sections_in_order() {
        let entries = menu_entries(MenuKind::Analyse);
        let subs: Vec<&str> = entries.iter().filter_map(|e| if let MenuEntry::Sub(n, _) = e { Some(*n) } else { None }).collect();
        assert_eq!(subs, vec!["Treasury & ALM", "Loans & credit", "Islamic window", "Reports", "Data checks"]);
        // every banking command is in a sub-menu
        for c in cmds::in_group(CommandGroup::Banking) {
            assert!(entries.iter().any(|e| matches!(e, MenuEntry::Sub(_, ids) if ids.contains(&c.id))), "{:?}", c.id);
        }
    }

    #[test]
    fn each_view_bar_has_groups_in_order_and_commands_that_exist() {
        for v in View::ALL {
            let bar = view_bar(v);
            // (the Dashboard view draws its own toolbar)
            assert!(!bar.is_empty() || v == View::Dashboard, "{v:?}");
            assert!(bar.windows(2).all(|w| w[0].1 <= w[1].1), "{v:?}: groups in order");
            for (item, _) in &bar {
                if let BarItem::Cmd(id) = item {
                    assert!(cmds::command(*id).is_some(), "{id:?}");
                }
            }
        }
        // the Data bar keeps every button the old bar had, in the same order
        let data: Vec<CommandId> = view_bar(View::Data).iter().filter_map(|(i, _)| if let BarItem::Cmd(id) = i { Some(*id) } else { None }).collect();
        for id in cmds::toolbar_buttons() {
            assert!(data.contains(&id), "{id:?}");
        }
        assert!(data.contains(&CommandId::SendToReport), "Send to report is on the Data bar");
    }

    #[test]
    fn a_command_runs_in_its_view() {
        assert_eq!(home_view(CommandId::GapLadder), Some(View::Data));
        assert_eq!(home_view(CommandId::Filter), Some(View::Data));
        assert_eq!(home_view(CommandId::Dashboard), Some(View::Dashboard));
        assert_eq!(home_view(CommandId::ModelKeyCheck), Some(View::Model));
        assert_eq!(home_view(CommandId::Quit), None);
        assert_eq!(home_view(CommandId::Pivot), None, "a pivot is made from wherever you are");
        assert_eq!(home_view(CommandId::ChartNew), None, "a chart is made from the table you are on; the view follows the new chart tab");
    }

    #[test]
    fn every_command_has_a_workflow_and_the_new_names_are_the_five() {
        let names: HashSet<&str> = cmds::COMMANDS.iter().map(|c| workflow_of(c.id)).collect();
        for n in ["File", "Data", "Analyse", "Visualise", "Share", "Settings", "Help", "Model", "Workspace"] {
            assert!(names.contains(n), "no command under {n}");
        }
        assert!(!names.contains(""), "a command without a workflow");
        assert_eq!(workflow_of(CommandId::GapLadder), "Analyse");
        assert_eq!(workflow_of(CommandId::Pivot), "Analyse");
        assert_eq!(workflow_of(CommandId::ExportCsv), "Share");
        assert_eq!(workflow_of(CommandId::Filter), "Data");
        assert_eq!(workflow_of(CommandId::ChartNew), "Visualise");
    }

    #[test]
    fn the_model_steps_say_what_is_done_and_what_is_wrong() {
        let none = model_steps(&ModelProgress::default());
        assert_eq!(none.len(), 6);
        assert_eq!(none[0].state, StepState::Todo);
        assert_eq!(first_open_step(&none), ModelStep::Tables);
        let p = ModelProgress { tables: 3, links: 2, checked: 2, broken: 1, unmatched: 1, calendar: false, as_of_chosen: false, measures: 0 };
        let s = model_steps(&p);
        assert_eq!(s[1].state, StepState::Done);
        assert_eq!((s[2].state.clone(), s[2].note.as_str()), (StepState::Problem, "2 problems"));
        assert_eq!(s[3].state, StepState::Todo);
        assert_eq!(s[4].state, StepState::Optional);
        assert_eq!(first_open_step(&s), ModelStep::KeyCheck, "the first step that needs work, a problem included");
        let ok = ModelProgress { tables: 3, links: 2, checked: 2, broken: 0, unmatched: 0, calendar: true, as_of_chosen: false, measures: 1 };
        let s = model_steps(&ok);
        assert!(s.iter().all(|i| i.state == StepState::Done), "{s:?}");
        assert_eq!(s[2].note, "All keys match");
        assert_eq!(s[4].note, "Latest data date");
        // a key check that has not run is still to do
        let unchecked = ModelProgress { tables: 2, links: 1, checked: 0, ..Default::default() };
        assert_eq!(model_steps(&unchecked)[2].state, StepState::Todo);
    }

    #[test]
    fn steps_move_back_and_next() {
        assert_eq!(ModelStep::Tables.previous(), None);
        assert_eq!(ModelStep::Tables.next(), Some(ModelStep::Links));
        assert_eq!(ModelStep::KeyCheck.previous(), Some(ModelStep::Links));
        assert_eq!(ModelStep::Measures.next(), None);
    }

    fn link(d: &str, l: &str, status: &str, bad: bool) -> DiagramLink {
        DiagramLink { data_key: d.into(), data_title: d.to_uppercase(), lookup_key: l.into(), lookup_title: l.to_uppercase(), status: status.into(), bad }
    }

    #[test]
    fn the_diagram_puts_data_tables_left_and_lookups_to_their_right() {
        let d = layout_diagram(&[link("tb", "branch", "Matched", false), link("tb", "product", "2 duplicate keys", true), link("budget", "branch", "Matched", false)], Some(("cal", "Calendar")), &[("tb".into(), "TB".into())]);
        let at = |k: &str| d.nodes.iter().find(|n| n.key == k).map(|n| (n.column, n.row)).unwrap_or((99, 99));
        assert_eq!(at("tb"), (0, 0));
        assert_eq!(at("budget"), (0, 1));
        assert_eq!(at("branch"), (1, 0));
        assert_eq!(at("product"), (1, 1));
        assert_eq!(at("cal"), (1, 2), "the calendar is a lookup of the table with a date link");
        assert_eq!((d.columns, d.rows), (2, 3));
        assert_eq!(d.edges.len(), 4);
        assert!(d.edges.iter().any(|e| e.bad && e.to == "product"));
        assert_eq!(d.edges.last().map(|e| e.status.as_str()), Some("Date link"));
    }

    #[test]
    fn a_chain_of_lookups_gets_a_column_each_and_a_loop_cannot_hang_the_layout() {
        let chain = layout_diagram(&[link("a", "b", "", false), link("b", "c", "", false)], None, &[]);
        let col = |k: &str| chain.nodes.iter().find(|n| n.key == k).map(|n| n.column);
        assert_eq!((col("a"), col("b"), col("c")), (Some(0), Some(1), Some(2)));
        let looped = layout_diagram(&[link("a", "b", "", false), link("b", "a", "", false)], None, &[]);
        assert_eq!(looped.nodes.len(), 2);
        assert!(layout_diagram(&[], None, &[]).nodes.is_empty());
    }

    #[test]
    fn each_empty_view_says_what_to_do_next_and_the_buttons_do_something() {
        let none = HintState::default();
        let data = empty_hint(View::Data, none).expect("an empty Data view has a hint");
        assert_eq!(data.cards.iter().map(|c| c.action).collect::<Vec<_>>(), vec![HintAction::Run(CommandId::OpenFile), HintAction::Run(CommandId::Stack), HintAction::Run(CommandId::Batch)]);
        assert!(empty_hint(View::Data, HintState { data_tabs: 1, ..none }).is_none(), "a Data view with a table has no hint");
        let rep = empty_hint(View::Report, none).expect("report");
        assert_eq!(rep.cards[0].action, HintAction::Go(View::Data), "with no data the report points to the Data view");
        let rep = empty_hint(View::Report, HintState { data_tabs: 2, ..none }).expect("report with data");
        assert!(rep.cards.iter().any(|c| c.action == HintAction::Run(CommandId::SendToReport)));
        assert!(empty_hint(View::Report, HintState { report_tabs: 1, ..none }).is_none());
        assert!(empty_hint(View::Report, HintState { dashboard: true, ..none }).is_some(), "the dashboard is its own view: an empty Report still says what to do");
        assert!(empty_hint(View::Dashboard, none).is_none(), "the Dashboard view has its own empty-page cards");
        assert!(empty_hint(View::Model, none).is_some() && empty_hint(View::Model, HintState { links: 1, ..none }).is_none());
        for v in View::ALL {
            if let Some(h) = empty_hint(v, none) {
                assert!(!h.title.is_empty() && !h.line.is_empty() && h.cards.iter().all(|c| !c.button.is_empty() && !c.body.is_empty()));
            }
        }
        let dash = empty_dashboard_hint(3, 0);
        assert_eq!(dash.cards.len(), 3);
        assert_eq!(dash.cards[0].body, "Pick from the chart tabs that are open");
        assert_eq!(dash.cards[1].body, "No pivot tab is open yet");
    }

    #[test]
    fn the_empty_search_lists_only_commands_that_exist() {
        let r = recent_for_palette(&[CommandId::GapLadder, CommandId::Pivot, CommandId::ExportPdf, CommandId::Filter], 3);
        assert_eq!(r, vec![CommandId::GapLadder, CommandId::Pivot, CommandId::ExportPdf]);
        assert_eq!(TRY_TYPING.len(), 4);
    }
}
