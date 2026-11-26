//! Phase 3 T6: the command list and the Ctrl+K ranking.

use micro_parquet::commands::*;
use std::collections::HashSet;

fn e<'a>(label: &'a str, words: &'a str, recent: Option<usize>) -> Entry<'a> {
    Entry { label, words, recent }
}

fn order(query: &str, entries: &[Entry]) -> Vec<usize> {
    rank(query, entries)
}

#[test]
fn t6_a_word_that_starts_with_the_query_beats_a_match_inside_a_word() {
    let entries = [
        e("Profile summary", "", None),       // "fil" inside "profile"
        e("Filter rows", "", None),           // word starts with "fil"
        e("Re-file the batch", "", None),     // "file" starts with fil, second word
    ];
    let r = order("fil", &entries);
    assert_eq!(r[0], 1, "the word that starts with the query comes first");
    assert_eq!(*r.last().unwrap(), 0, "the match inside a word comes last");
    // an exact word beats a longer word that starts with it
    let entries = [e("Sorting rules", "", None), e("Sort rows", "", None)];
    assert_eq!(order("sort", &entries)[0], 1);
    // a label match beats a match only in the extra words
    let entries = [e("Pivot", "sort group", None), e("Sort rows", "", None)];
    assert_eq!(order("sort", &entries)[0], 1);
    // an earlier word in the label ranks higher than a later one
    let entries = [e("Export to Excel", "", None), e("Excel export", "", None)];
    assert_eq!(order("excel", &entries)[0], 1);
}

#[test]
fn t6_recently_used_breaks_ties_and_only_ties() {
    // equal matches: the one used more recently first, then list order
    let entries = [e("Export to CSV", "", None), e("Export to Parquet", "", Some(1)), e("Export to PDF", "", Some(0))];
    assert_eq!(order("export", &entries), vec![2, 1, 0]);
    // a better match beats a recent worse one
    let entries = [e("Reexport files", "", Some(0)), e("Export now", "", None)];
    assert_eq!(order("export", &entries), vec![1, 0]);
    // nothing typed: recently used first, then the list as it is
    let entries = [e("A", "", None), e("B", "", Some(1)), e("C", "", Some(0)), e("D", "", None)];
    assert_eq!(order("", &entries), vec![2, 1, 0, 3]);
    assert_eq!(order("   ", &entries), vec![2, 1, 0, 3]);
}

#[test]
fn t6_every_word_must_match_and_letters_in_order_still_find() {
    let entries = [
        e("Export tabs to an Excel workbook", "xlsx", None),
        e("Export this tab to Parquet", "", None),
        e("Filter rows", "", None),
    ];
    assert_eq!(order("export excel", &entries), vec![0]);
    assert_eq!(order("excel parquet", &entries), Vec::<usize>::new());
    assert_eq!(order("xlsx", &entries), vec![0], "extra words are searched");
    assert_eq!(order("flt", &entries), vec![2], "letters in order: flt finds Filter");
    assert!(order("zzz", &entries).is_empty());
    assert_eq!(order("EXPORT", &entries).len(), 2, "case does not matter");
}

#[test]
fn t6_the_real_list_finds_what_a_banker_would_type() {
    let labels: Vec<String> = COMMANDS.iter().map(|c| c.title.to_string()).collect();
    let words: Vec<String> = COMMANDS.iter().map(|c| format!("{} {}", c.words, c.group.label())).collect();
    let entries: Vec<Entry> = labels.iter().zip(&words).map(|(l, w)| e(l, w, None)).collect();
    let top = |q: &str| COMMANDS[order(q, &entries)[0]].id;
    assert_eq!(top("filter"), CommandId::Filter);
    assert_eq!(top("sort"), CommandId::Sort);
    // two commands are about Excel since Phase 7: the workbook of tabs and the chart workbook
    assert!([CommandId::ExportXlsx, CommandId::ExportChartXlsx].contains(&top("excel")));
    let excel: Vec<CommandId> = order("excel", &entries).iter().map(|i| COMMANDS[*i].id).collect();
    assert!(excel.contains(&CommandId::ExportXlsx), "the workbook of tabs is still found by 'excel'");
    assert_eq!(top("pdf"), CommandId::ExportPdf);
    assert_eq!(top("dark"), CommandId::ToggleTheme);
    assert_eq!(top("provision"), CommandId::Provision);
    assert_eq!(top("nii"), CommandId::RateShock);
    assert_eq!(top("eve"), CommandId::Duration);
    assert_eq!(top("lakh"), CommandId::ScaleLakh);
    assert_eq!(top("macro"), CommandId::SaveMacro);
    assert_eq!(top("undo"), CommandId::Undo);
    assert_eq!(top("ftp"), CommandId::Ftp);
    // a typed column name: the "Column: ..." entry is the match
    let mut labels2 = labels.clone();
    labels2.push("Column: Balance".to_string());
    let mut words2 = words.clone();
    words2.push("column field inspector statistics".to_string());
    let entries2: Vec<Entry> = labels2.iter().zip(&words2).map(|(l, w)| e(l, w, None)).collect();
    assert_eq!(order("balance", &entries2)[0], labels2.len() - 1);
}

#[test]
fn t6_every_toolbar_action_is_in_the_command_list() {
    let ids: HashSet<CommandId> = COMMANDS.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), COMMANDS.len(), "no command is listed twice");
    let titles: HashSet<&str> = COMMANDS.iter().map(|c| c.title).collect();
    assert_eq!(titles.len(), COMMANDS.len(), "titles are unique, so the search can tell them apart");
    let toolbar = toolbar_commands();
    for id in &toolbar {
        assert!(ids.contains(id), "{id:?} is on the toolbar but not in the command list");
        assert!(command(*id).is_some());
    }
    // the toolbar of the design: Open / Stack / Batch | Filter / Sort / Columns / Calc column / Pivot
    for want in [
        CommandId::OpenFile, CommandId::Stack, CommandId::Batch, CommandId::Filter, CommandId::Sort,
        CommandId::Columns, CommandId::CalcColumn, CommandId::Pivot,
    ] {
        assert!(toolbar.contains(&want), "{want:?}");
    }
    // each banking tool and each export type
    let banking: Vec<CommandId> = in_group(CommandGroup::Banking).map(|c| c.id).collect();
    for want in [
        CommandId::Buckets, CommandId::RateShock, CommandId::Duration, CommandId::Ftp,
        CommandId::Provision, CommandId::Limits, CommandId::Reconcile,
    ] {
        assert!(banking.contains(&want) && toolbar.contains(&want), "{want:?}");
    }
    // the hygiene audit and the data-quality scorecard are in Edit ▸ Data checks (F3), not in Banking
    let edit: Vec<CommandId> = in_group(CommandGroup::Edit).map(|c| c.id).collect();
    for want in [CommandId::Audit, CommandId::DqScorecard] {
        assert!(edit.contains(&want) && !banking.contains(&want) && toolbar.contains(&want) && DATA_CHECKS.contains(&want), "{want:?}");
    }
    let export: Vec<CommandId> = in_group(CommandGroup::Export).map(|c| c.id).collect();
    for want in [CommandId::ExportParquet, CommandId::ExportCsv, CommandId::ExportXlsx, CommandId::ExportPdf, CommandId::ExportSplit] {
        assert!(export.contains(&want) && toolbar.contains(&want), "{want:?}");
    }
    // menu items and the theme toggle
    for want in [
        CommandId::ToggleTheme, CommandId::Quit, CommandId::About, CommandId::SaveMacro, CommandId::LoadMacro,
        CommandId::PythonExport, CommandId::DirectConvert, CommandId::Undo, CommandId::Redo,
    ] {
        assert!(ids.contains(&want), "{want:?}");
    }
    // the app draws its toolbar from this list
    let main = std::fs::read_to_string("src/ui/top_bar.rs").unwrap(); // the single top bar draws it
    // (the bar is drawn from the view's `view_bar`, whose Data bar a unit test checks against toolbar_buttons(); the menus are File, Settings, Help)
    assert!(main.contains("view_bar") && main.contains("MENUS"));
    let ws = std::fs::read_to_string("src/workspace.rs").unwrap();
    assert!(ws.contains("CommandGroup::Banking") && ws.contains("MenuKind::ShareData"));
}

#[test]
fn t6_unavailable_commands_say_why() {
    let none = CommandState { tabs: 0, can_undo: false, can_redo: false, ..Default::default() };
    let one = CommandState { tabs: 1, can_undo: false, can_redo: false, ..Default::default() };
    let two = CommandState { tabs: 2, can_undo: true, can_redo: true, ..Default::default() };
    let reason = |id: CommandId, s: CommandState| unavailable_reason(command(id).unwrap(), s);
    assert_eq!(reason(CommandId::OpenFile, none), None, "opening a file always works");
    assert_eq!(reason(CommandId::ToggleTheme, none), None);
    assert_eq!(reason(CommandId::Filter, none), Some("Open a file first."));
    assert_eq!(reason(CommandId::Filter, one), None);
    assert_eq!(reason(CommandId::ExportXlsx, none), Some("Open a file first."));
    assert_eq!(reason(CommandId::JoinTabs, one), Some("Needs two open tabs."));
    assert_eq!(reason(CommandId::Reconcile, one), Some("Needs two open tabs."));
    assert_eq!(reason(CommandId::JoinTabs, two), None);
    assert_eq!(reason(CommandId::Undo, one), Some("Nothing to undo."));
    assert_eq!(reason(CommandId::Redo, one), Some("Nothing to redo."));
    assert_eq!(reason(CommandId::Undo, two), None);
    // every command that needs something has a reason when nothing is open
    for c in COMMANDS {
        if c.needs != Needs::Nothing {
            assert!(unavailable_reason(c, none).is_some(), "{:?}", c.id);
        } else {
            assert!(unavailable_reason(c, none).is_none(), "{:?}", c.id);
        }
    }
}

#[test]
fn t6_recently_used_is_kept_without_duplicates_and_saved_as_text() {
    let mut r: Vec<CommandId> = Vec::new();
    note_used(&mut r, CommandId::Filter, 3);
    note_used(&mut r, CommandId::Sort, 3);
    note_used(&mut r, CommandId::Filter, 3);
    assert_eq!(r, vec![CommandId::Filter, CommandId::Sort]);
    note_used(&mut r, CommandId::Pivot, 3);
    note_used(&mut r, CommandId::Columns, 3);
    assert_eq!(r.len(), 3);
    assert_eq!(r[0], CommandId::Columns);
    let text = recent_to_text(&r);
    assert_eq!(recent_from_text(&text), r);
    // names that no longer exist are dropped
    assert_eq!(recent_from_text("Filter\nRetiredCommand\nSort\n\n"), vec![CommandId::Filter, CommandId::Sort]);
}
