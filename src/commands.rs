//! The command list behind the toolbar, the menus and the Ctrl+K search (Phase 3 T6): every
//! action with its title, group and search words, which of them are available in a given state,
//! and how a typed query ranks them.

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CommandId {
    // File
    OpenFile,
    DirectConvert,
    Batch,
    Stack,
    SaveMacro,
    LoadMacro,
    LoadMacroNewTab,
    PythonExport,
    Quit,
    // Edit
    Undo,
    Redo,
    // Transform
    Filter,
    Sort,
    Columns,
    CalcColumn,
    Pivot,
    Conditional,
    Normaliser,
    CleanBreaks,
    JoinTabs,
    CopyTable,
    // Banking
    Buckets,
    GapLadder,
    IrrbbLadder,
    BehaviourProfile,
    RateProfile,
    RateShock,
    Duration,
    Ftp,
    YieldCurves,
    Provision,
    Limits,
    Reconcile,
    ClMigration,
    Audit,
    // Export
    ExportParquet,
    ExportCsv,
    ExportXlsx,
    ExportPdf,
    ExportSplit,
    ParquetArchive,
    ParquetSnappy,
    // View
    ToggleTheme,
    ZoomIn,
    ZoomOut,
    ScaleCrore,
    ScaleLakh,
    ScaleMillion,
    ScaleThousand,
    ScalePlain,
    ScaleRaw,
    DecimalsUp,
    DecimalsDown,
    TogglePanel,
    ToggleSql,
    ToggleChart,
    ToggleSummary,
    // Help
    About,
}

/// Where a command lives in the menus and toolbar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandGroup {
    File,
    Edit,
    Transform,
    Banking,
    Export,
    View,
    Help,
}

impl CommandGroup {
    pub fn label(&self) -> &'static str {
        match self {
            CommandGroup::File => "File",
            CommandGroup::Edit => "Edit",
            CommandGroup::Transform => "Transform",
            CommandGroup::Banking => "Banking",
            CommandGroup::Export => "Export",
            CommandGroup::View => "View",
            CommandGroup::Help => "Help",
        }
    }
}

/// What a command needs before it can run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Needs {
    Nothing,
    /// An open tab.
    Tab,
    /// At least two open tabs.
    TwoTabs,
    /// Something to undo on the active tab.
    Undo,
    /// Something to redo on the active tab.
    Redo,
}

#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub id: CommandId,
    pub title: &'static str,
    pub group: CommandGroup,
    /// Extra words the search matches ("excel" finds the Excel export).
    pub words: &'static str,
    pub needs: Needs,
}

const fn c(id: CommandId, title: &'static str, group: CommandGroup, words: &'static str, needs: Needs) -> Command {
    Command { id, title, group, words, needs }
}

use CommandGroup as G;
use CommandId as I;
use Needs as N;

/// Every command, in menu order.
pub const COMMANDS: &[Command] = &[
    c(I::OpenFile, "Open CSV or Parquet file\u{2026}", G::File, "load import browse", N::Nothing),
    c(I::DirectConvert, "Convert CSV to Parquet directly\u{2026}", G::File, "converter transform file", N::Nothing),
    c(I::Batch, "Recursive batch folder mirror\u{2026}", G::File, "folder convert many files", N::Nothing),
    c(I::Stack, "Stack / merge several files\u{2026}", G::File, "append union combine vertical", N::Nothing),
    c(I::SaveMacro, "Save steps as a macro\u{2026}", G::File, "recipe mpq json", N::Tab),
    c(I::LoadMacro, "Load and replay a macro on this tab\u{2026}", G::File, "recipe mpq json apply", N::Tab),
    c(I::LoadMacroNewTab, "Load and replay a macro into a new tab\u{2026}", G::File, "recipe mpq json apply", N::Tab),
    c(I::PythonExport, "Export the steps as a Python script\u{2026}", G::File, "py polars code", N::Tab),
    c(I::Quit, "Quit", G::File, "exit close", N::Nothing),
    c(I::Undo, "Undo the last change to the steps", G::Edit, "ctrl z back", N::Undo),
    c(I::Redo, "Redo the change to the steps", G::Edit, "ctrl y forward", N::Redo),
    c(I::Filter, "Filter rows\u{2026}", G::Transform, "condition where keep delete", N::Tab),
    c(I::Sort, "Sort rows\u{2026}", G::Transform, "order ascending descending", N::Tab),
    c(I::Columns, "Columns: drop, order, rename, format\u{2026}", G::Transform, "select hide rename decimals", N::Tab),
    c(I::CalcColumn, "Calc column\u{2026}", G::Transform, "formula math add divide new", N::Tab),
    c(I::Pivot, "Pivot table (Rows, Columns, Values)\u{2026}", G::Transform, "group by aggregate wavg weighted average summary crosstab subtotal", N::Tab),
    c(I::Conditional, "Conditional column (IF / ELSE)\u{2026}", G::Transform, "case when flag", N::Tab),
    c(I::CleanBreaks, "Clean line breaks\u{2026}", G::Transform, "newline carriage return quoted multiline cell wrap spaces replace", N::Tab),
    c(I::Normaliser, "Counterparty name normaliser\u{2026}", G::Transform, "clean entity names ltd", N::Tab),
    c(I::JoinTabs, "SQL join of two open tabs\u{2026}", G::Transform, "merge lookup left right", N::TwoTabs),
    c(I::CopyTable, "Copy the table to the clipboard", G::Transform, "excel powerpoint slides paste spreadsheet", N::Tab),
    c(I::Buckets, "Maturity bucketer (SLP / LCR / custom)\u{2026}", G::Banking, "alm gap liquidity tenor", N::Tab),
    c(I::GapLadder, "Liquidity gap ladder\u{2026}", G::Banking, "alm gap slp cumulative inflow outflow mismatch limit buckets", N::Tab),
    c(I::IrrbbLadder, "IRRBB cash-flow ladder\u{2026}", G::Banking, "irrbb cash flow ladder bucket repricing eve nmd prepayment cpr tdrr behaviour schedule instalment", N::Tab),
    c(I::BehaviourProfile, "Behaviour profile (core and volatile deposits)\u{2026}", G::Banking, "behavioural behavioral casa nmd non-maturity deposit savings current core volatile cpr tdrr prepayment early redemption irrbb caps", N::Nothing),
    c(I::RateProfile, "Rate profile (rate bands)\u{2026}", G::Banking, "rate band bucket interest lending deposit yield cost profile", N::Tab),
    c(I::RateShock, "Interest rate shock / \u{394}NII\u{2026}", G::Banking, "nii irrbb bps delta income", N::Tab),
    c(I::Duration, "Duration, DV01 and \u{394}EVE\u{2026}", G::Banking, "eve irrbb modified macaulay", N::Tab),
    c(I::Ftp, "FTP yield curve mapper\u{2026}", G::Banking, "funds transfer pricing margin", N::Tab),
    c(I::YieldCurves, "Yield curves (dated FTP curves)\u{2026}", G::Banking, "curve ftp tenor store csv import dated origination", N::Nothing),
    c(I::Provision, "Classified-loan provision and BFP\u{2026}", G::Banking, "cl npl bangladesh bank base", N::Tab),
    c(I::Limits, "Limit utilisation, over-limit and undrawn CCF\u{2026}", G::Banking, "credit exposure", N::Tab),
    c(I::Reconcile, "Two-period variance and CL migration\u{2026}", G::Banking, "compare previous current movement", N::TwoTabs),
    c(I::ClMigration, "CL migration matrix\u{2026}", G::Banking, "classified loans grade upgrade downgrade npl roll rate movement transition", N::TwoTabs),
    c(I::Audit, "Core banking dirty-data auditor\u{2026}", G::Banking, "hygiene ghost exceptions quality", N::Tab),
    c(I::ExportParquet, "Export this tab to Parquet\u{2026}", G::Export, "save parquet", N::Tab),
    c(I::ExportCsv, "Export this tab to CSV\u{2026}", G::Export, "save csv", N::Tab),
    c(I::ExportXlsx, "Export tabs to an Excel workbook\u{2026}", G::Export, "xlsx save spreadsheet totals", N::Tab),
    c(I::ExportPdf, "Export tabs to a PDF report\u{2026}", G::Export, "save print", N::Tab),
    c(I::ExportSplit, "Split export by column\u{2026}", G::Export, "partition branch files", N::Tab),
    c(I::ParquetArchive, "Parquet compression: archive (Zstd)", G::Export, "smaller", N::Nothing),
    c(I::ParquetSnappy, "Parquet compression: Power BI (Snappy)", G::Export, "faster powerbi", N::Nothing),
    c(I::ToggleTheme, "Switch light / dark theme", G::View, "dark mode night appearance", N::Nothing),
    c(I::ZoomIn, "Zoom in (UI scale)", G::View, "bigger larger", N::Nothing),
    c(I::ZoomOut, "Zoom out (UI scale)", G::View, "smaller", N::Nothing),
    c(I::ScaleCrore, "Show money in Crore", G::View, "scale cr 10000000", N::Nothing),
    c(I::ScaleLakh, "Show money in Lakh", G::View, "scale lac 100000", N::Nothing),
    c(I::ScaleMillion, "Show money in Millions", G::View, "scale mn m 1000000", N::Nothing),
    c(I::ScaleThousand, "Show money in Thousands", G::View, "scale k 1000", N::Nothing),
    c(I::ScalePlain, "Show money as plain numbers with commas", G::View, "scale commas", N::Nothing),
    c(I::ScaleRaw, "Show money as raw numbers", G::View, "scale unformatted", N::Nothing),
    c(I::DecimalsUp, "More decimal places", G::View, "precision", N::Nothing),
    c(I::DecimalsDown, "Fewer decimal places", G::View, "precision", N::Nothing),
    c(I::TogglePanel, "Show or hide the right panel", G::View, "steps column inspector sidebar", N::Nothing),
    c(I::ToggleSql, "Show or hide the SQL scratchpad bar", G::View, "query", N::Nothing),
    c(I::ToggleChart, "Show or hide the ALCO bar chart", G::View, "graph plot", N::Nothing),
    c(I::ToggleSummary, "Show/hide summary row", G::View, "totals sum count average footer selectors band", N::Nothing),
    c(I::About, "About MicroParquet", G::Help, "version data folder path", N::Nothing),
];

/// The heading a Banking command is listed under in the menu (the Banking menu lists them in
/// order, so the commands of one heading are next to each other).
pub fn banking_section(id: CommandId) -> Option<&'static str> {
    match id {
        CommandId::Buckets | CommandId::GapLadder | CommandId::IrrbbLadder | CommandId::BehaviourProfile | CommandId::RateProfile | CommandId::RateShock | CommandId::Duration | CommandId::Ftp | CommandId::YieldCurves => Some("Treasury & ALM"),
        CommandId::Provision | CommandId::Limits | CommandId::Reconcile | CommandId::ClMigration | CommandId::Audit => Some("Loans & credit"),
        _ => None,
    }
}

pub fn command(id: CommandId) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// The commands of one group, in menu order.
pub fn in_group(group: CommandGroup) -> impl Iterator<Item = &'static Command> {
    COMMANDS.iter().filter(move |c| c.group == group)
}

/// The toolbar: groups of buttons, then the menus (their items are the commands of that group).
pub const TOOLBAR_BUTTONS_FILE: &[CommandId] = &[I::OpenFile, I::Stack, I::Batch];
pub const TOOLBAR_BUTTONS_TRANSFORM: &[CommandId] = &[I::Filter, I::Sort, I::Columns, I::CalcColumn, I::Pivot];
/// Items of the toolbar's "More" menu.
pub const TOOLBAR_MORE: &[CommandId] = &[I::Conditional, I::Normaliser, I::JoinTabs, I::ToggleSql, I::ToggleChart, I::CopyTable];

/// Every command the toolbar can reach: its buttons, the More menu, and the Banking and Export
/// menus.
pub fn toolbar_commands() -> Vec<CommandId> {
    let mut v: Vec<CommandId> = Vec::new();
    v.extend_from_slice(TOOLBAR_BUTTONS_FILE);
    v.extend_from_slice(TOOLBAR_BUTTONS_TRANSFORM);
    v.extend_from_slice(TOOLBAR_MORE);
    v.extend(in_group(CommandGroup::Banking).map(|c| c.id));
    v.extend(in_group(CommandGroup::Export).map(|c| c.id));
    v
}

/// What the app looks like right now, as far as availability goes.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandState {
    pub tabs: usize,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Why a command cannot run now; `None` when it can.
pub fn unavailable_reason(cmd: &Command, state: CommandState) -> Option<&'static str> {
    match cmd.needs {
        Needs::Nothing => None,
        Needs::Tab if state.tabs == 0 => Some("Open a file first."),
        Needs::TwoTabs if state.tabs < 2 => Some("Needs two open tabs."),
        Needs::Undo if state.tabs == 0 || !state.can_undo => Some("Nothing to undo."),
        Needs::Redo if state.tabs == 0 || !state.can_redo => Some("Nothing to redo."),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------ ranking

/// How well `word` matches `text` (higher is better): a word that starts with it is best, then a
/// text that starts with it, then a match inside a word, then the letters in order ("flt" finds
/// "Filter"). `None` when it does not match.
pub fn word_score(word: &str, text: &str) -> Option<u32> {
    let w = word.to_lowercase();
    if w.is_empty() {
        return Some(0);
    }
    let t = text.to_lowercase();
    let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
    if let Some(pos) = words.iter().position(|x| *x == w) {
        // an earlier word in the text ranks higher
        return Some(1000 - (pos as u32).min(20) * 5);
    }
    if words.iter().any(|x| x.starts_with(&w)) {
        // earlier words rank higher
        let pos = words.iter().position(|x| x.starts_with(&w)).unwrap_or(0) as u32;
        return Some(900 - pos.min(20) * 5);
    }
    if t.contains(&w) {
        return Some(300);
    }
    // letters in order, all inside the text
    let mut it = t.chars();
    if w.chars().all(|ch| it.any(|x| x == ch)) {
        return Some(100);
    }
    None
}

/// A searchable entry: the text shown, extra words, and when it was last used (0 = most recent).
pub struct Entry<'a> {
    pub label: &'a str,
    pub words: &'a str,
    pub recent: Option<usize>,
}

/// The score of a whole query against an entry: every word of the query must match the label or
/// the extra words (label matches count double). `None` when one does not.
pub fn query_score(query: &str, entry: &Entry) -> Option<u32> {
    let mut total = 0u32;
    for word in query.split_whitespace() {
        let in_label = word_score(word, entry.label).map(|s| s * 2);
        let in_words = word_score(word, entry.words);
        total += in_label.into_iter().chain(in_words).max()?;
    }
    Some(total)
}

/// Indexes of `entries` in the order to show them. A better match comes first; equal matches are
/// ordered by how recently they were used, then by their place in the list. With an empty query,
/// the recently used come first and the rest keep their order.
pub fn rank(query: &str, entries: &[Entry]) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| query_score(query, e).map(|s| (i, s)))
        .collect();
    scored.sort_by(|(ia, sa), (ib, sb)| {
        let (ra, rb) = (entries[*ia].recent.unwrap_or(usize::MAX), entries[*ib].recent.unwrap_or(usize::MAX));
        sb.cmp(sa).then(ra.cmp(&rb)).then(ia.cmp(ib))
    });
    scored.into_iter().map(|(i, _)| i).collect()
}

/// Puts `id` first in the list of recently used commands (no duplicates, at most `keep`).
pub fn note_used(recent: &mut Vec<CommandId>, id: CommandId, keep: usize) {
    recent.retain(|x| *x != id);
    recent.insert(0, id);
    recent.truncate(keep);
}

/// The recently used commands as text, for saving.
pub fn recent_to_text(recent: &[CommandId]) -> String {
    recent.iter().map(|i| format!("{:?}", i)).collect::<Vec<_>>().join("\n")
}

/// The recently used commands from saved text; names that no longer exist are dropped.
pub fn recent_from_text(text: &str) -> Vec<CommandId> {
    let by_name: HashMap<String, CommandId> = COMMANDS.iter().map(|c| (format!("{:?}", c.id), c.id)).collect();
    text.lines().filter_map(|l| by_name.get(l.trim()).copied()).collect()
}
