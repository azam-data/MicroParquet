//! C2: the chart gallery, "Suggest a chart", saved charts and the Charts menu's commands.

use micro_parquet::chart::{ChartAgg, ChartKind};
use micro_parquet::charts::gallery::{apply_card, card_by_id, cards_for, fit_columns, Question, CARDS};
use micro_parquet::charts::library::{chart_path, exists, list_charts, load_chart, missing_columns, save_chart};
use micro_parquet::charts::spec::{ChartSpec, MeasureSpec, RefLine, Stack, Status, Transform};
use micro_parquet::charts::suggest::{looks_ordered, suggest, ColKind, SuggestCol, SuggestFlags};
use micro_parquet::commands::{command, in_group, unavailable_reason, CommandGroup, CommandId, CommandState};

fn col(name: &str, kind: ColKind, ordered: bool) -> SuggestCol {
    SuggestCol { name: name.into(), kind, ordered }
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mp_p7c2_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

// ----------------------------------------------------------------------------------- suggestions

#[test]
fn c2_each_suggestion_rule_gives_its_form_and_a_reason() {
    let f = SuggestFlags::default();
    // date + number -> line
    let s = suggest(&[col("Date", ColKind::Date, false), col("Balance", ColKind::Number, false)], f).unwrap();
    assert_eq!((s.kind, s.category.as_str(), s.value.as_deref()), (ChartKind::Line, "Date", Some("Balance")));
    assert!(s.reason.contains("trend"));
    // text + number -> sorted bar
    let s = suggest(&[col("Branch", ColKind::Text, false), col("Balance", ColKind::Number, false)], f).unwrap();
    assert_eq!((s.kind, s.horizontal, s.by_value, s.category.as_str()), (ChartKind::Bar, true, true, "Branch"));
    assert!(s.reason.contains("sorted"));
    // ordered text (buckets, tenors, grades) + number -> column in that order
    let s = suggest(&[col("Bucket", ColKind::Text, true), col("Gap", ColKind::Number, false)], f).unwrap();
    assert_eq!((s.kind, s.horizontal, s.by_value, s.keep_order), (ChartKind::Bar, false, false, true));
    assert!(s.reason.contains("order"));
    // two numbers -> scatter
    let s = suggest(&[col("Tenor", ColKind::Number, false), col("Rate", ColKind::Number, false)], f).unwrap();
    assert_eq!((s.kind, s.category.as_str(), s.value.as_deref()), (ChartKind::Scatter, "Tenor", Some("Rate")));
    // parts of a total over time -> stacked column
    let s = suggest(&[col("Month", ColKind::Date, false), col("Product", ColKind::Text, false), col("Balance", ColKind::Number, false)], f).unwrap();
    assert_eq!((s.kind, s.stack, s.series.as_deref(), s.category.as_str()), (ChartKind::Bar, Stack::Stacked, Some("Product"), "Month"));
    // two text columns + number -> heatmap
    let s = suggest(&[col("Previous", ColKind::Text, true), col("Current", ColKind::Text, true), col("Balance", ColKind::Number, false)], f).unwrap();
    assert_eq!((s.kind, s.category.as_str(), s.series.as_deref()), (ChartKind::Heatmap, "Previous", Some("Current")));
    // a signed change between two totals -> waterfall
    let s = suggest(&[col("Step", ColKind::Text, false), col("Amount", ColKind::Number, false)], SuggestFlags { change_between_totals: true }).unwrap();
    assert_eq!((s.kind, s.category.as_str()), (ChartKind::Waterfall, "Step"));
    assert!(s.reason.contains("start total"));
}

#[test]
fn c2_a_suggestion_with_the_wrong_columns_says_what_is_missing() {
    let f = SuggestFlags::default();
    assert!(suggest(&[], f).unwrap_err().contains("at least one column"));
    assert!(suggest(&[col("Branch", ColKind::Text, false)], f).unwrap_err().contains("number column"));
    assert!(suggest(&[col("Balance", ColKind::Number, false)], f).unwrap_err().contains("category"));
    assert!(suggest(&[col("Balance", ColKind::Number, false)], SuggestFlags { change_between_totals: true }).unwrap_err().contains("waterfall needs"));
}

#[test]
fn c2_a_text_column_is_ordered_when_its_name_or_values_say_so() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(looks_ordered("Maturity Bucket", &[]));
    assert!(looks_ordered("X", &v(&["1M", "3M", "6M", "1Y", "2Y"])));
    assert!(looks_ordered("X", &v(&["Jan", "Feb", "Mar"])));
    assert!(!looks_ordered("Branch", &v(&["Dhaka", "Chattogram", "Sylhet"])));
    assert!(!looks_ordered("X", &v(&["only"])));
}

// ------------------------------------------------------------------------------------ the gallery

#[test]
fn c2_the_gallery_has_the_groups_and_cards_of_the_spec_each_with_a_use_and_an_avoid_line() {
    let names = |q: Question| cards_for(q).map(|c| c.name).collect::<Vec<_>>();
    assert_eq!(names(Question::Compare), ["Bar", "Grouped bars", "Sorted bar"]);
    assert_eq!(names(Question::Change), ["Line", "Area", "Column by month"]);
    assert_eq!(names(Question::Composition), ["Stacked column", "100% stacked", "Donut", "Waterfall"]);
    assert_eq!(names(Question::Distribution), ["Histogram", "Box plot"]);
    assert_eq!(names(Question::Relationship), ["Scatter", "Bubble", "Heatmap"]);
    assert_eq!(names(Question::FlowAndShare), ["Sankey", "Treemap"]);
    assert_eq!(CARDS.len(), 17);
    for c in CARDS {
        assert!(c.use_when.ends_with('.') && c.avoid_when.ends_with('.') && c.use_when.len() > 20 && c.avoid_when.len() > 20, "{}", c.id);
        assert_eq!(c.kind.html_only(), c.question == Question::FlowAndShare, "{}", c.id);
    }
    // the ids are unique
    let mut ids: Vec<&str> = CARDS.iter().map(|c| c.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), CARDS.len());
}

#[test]
fn c2_a_card_sets_the_form_and_the_columns_it_needs_are_filled_from_the_tab() {
    let cols = vec![col("Branch", ColKind::Text, false), col("Product", ColKind::Text, false), col("Balance", ColKind::Number, false), col("Rate", ColKind::Number, false), col("Limit", ColKind::Number, false)];
    let mut s = ChartSpec { category: "Branch".into(), value: Some("Balance".into()), ..Default::default() };
    apply_card(&mut s, card_by_id("sorted-bar").unwrap());
    assert_eq!((s.kind, s.horizontal, s.by_value), (ChartKind::Bar, true, true));
    apply_card(&mut s, card_by_id("stacked").unwrap());
    assert_eq!((s.kind, s.stack, s.horizontal), (ChartKind::Bar, Stack::Stacked, false));
    // scatter: numbers on both axes
    apply_card(&mut s, card_by_id("scatter").unwrap());
    fit_columns(&mut s, &cols);
    assert_eq!((s.kind, s.category.as_str(), s.value.as_deref()), (ChartKind::Scatter, "Balance", Some("Rate")));
    apply_card(&mut s, card_by_id("bubble").unwrap());
    fit_columns(&mut s, &cols);
    assert_eq!(s.size.as_ref().and_then(|m| m.column.as_deref()), Some("Limit"));
    // histogram: one number
    apply_card(&mut s, card_by_id("histogram").unwrap());
    fit_columns(&mut s, &cols);
    assert_eq!((s.category.as_str(), s.value.as_deref()), ("Rate", Some("Rate")), "the number already chosen is kept");
    // heatmap: two texts and "% of row"
    let mut h = ChartSpec::default();
    apply_card(&mut h, card_by_id("heatmap").unwrap());
    fit_columns(&mut h, &cols);
    assert_eq!((h.category.as_str(), h.series.as_deref(), h.value.as_deref()), ("Branch", Some("Product"), Some("Balance")));
    assert_eq!(h.measures[0].transform, Transform::ShareOfSeries);
}

// ------------------------------------------------------------------------------------ saved charts

fn rich_spec() -> ChartSpec {
    let mut s = ChartSpec { category: "Month".into(), series: Some("Product".into()), value: Some("Balance".into()), kind: ChartKind::Bar, stack: Stack::Stacked, horizontal: false, title: "T".into(), subtitle: "S".into(), footnote: "F".into(), takeaway: "Term deposits drove growth".into(), highlight: Some("Savings".into()), top_n: 7, ..Default::default() };
    s.ref_lines = vec![RefLine { value: -15.0, label: "Outlier".into(), status: Status::Critical, panel: 0 }];
    s.measures = vec![MeasureSpec { column: Some("Balance".into()), agg: ChartAgg::Sum, transform: Transform::ShareOfTotal, ..Default::default() }, MeasureSpec { column: Some("Rate".into()), agg: ChartAgg::WeightedAvg, weight: Some("Balance".into()), label: "Rate".into(), ..Default::default() }];
    s.colour_map.assign_all(&["Term deposits", "Savings", "Current"]);
    s.advanced_json = "{\"title\":{\"left\":\"center\"}}".into();
    s
}

#[test]
fn c2_a_saved_chart_reloads_identically_and_lists_by_name() {
    let dir = tmp("save");
    let spec = rich_spec();
    let path = save_chart(&dir, "Deposit mix", &spec).unwrap();
    assert_eq!(path, chart_path(&dir, "Deposit mix"));
    assert!(path.ends_with("Deposit mix.json") && exists(&dir, "Deposit mix") && !exists(&dir, "Other"));
    let (name, back) = load_chart(&path).unwrap();
    assert_eq!(name, "Deposit mix");
    assert_eq!(back, spec, "the chart reloads identically");
    save_chart(&dir, "Alpha", &ChartSpec::default()).unwrap();
    assert_eq!(list_charts(&dir).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["Alpha", "Deposit mix"]);
    // a file name that Windows would refuse is made safe
    let p = save_chart(&dir, "a/b: c?", &spec).unwrap();
    assert!(p.parent() == Some(dir.as_path()) && load_chart(&p).unwrap().0 == "a/b: c?");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c2_a_blank_name_a_damaged_file_and_a_newer_file_are_said_in_plain_words() {
    let dir = tmp("bad");
    assert!(save_chart(&dir, "   ", &ChartSpec::default()).unwrap_err().to_string().contains("Give the chart a name"));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("broken.json");
    std::fs::write(&bad, "{ not json").unwrap();
    let e = format!("{:#}", load_chart(&bad).unwrap_err());
    assert!(e.contains("broken.json") && e.contains("damaged") && e.contains("line 1"), "{e}");
    let newer = dir.join("newer.json");
    std::fs::write(&newer, "{\"version\": 99, \"name\": \"n\", \"chart\": {}}").unwrap();
    assert!(load_chart(&newer).unwrap_err().to_string().contains("newer MicroParquet"));
    assert!(format!("{:#}", load_chart(&dir.join("missing.json")).unwrap_err()).contains("Could not read"));
    // an old file with only some fields loads with the defaults
    let old = dir.join("old.json");
    std::fs::write(&old, "{\"chart\": {\"category\": \"A\"}}").unwrap();
    assert_eq!(load_chart(&old).unwrap().1.category, "A");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c2_a_chart_that_uses_columns_the_tab_lacks_names_them() {
    let spec = rich_spec();
    let have: Vec<String> = ["Month", "Product", "Balance"].map(String::from).to_vec();
    assert_eq!(missing_columns(&spec, &have), vec!["Rate".to_string()]);
    assert!(missing_columns(&spec, &["Month", "Product", "Balance", "Rate"].map(String::from)).is_empty());
}

// ------------------------------------------------------------------------------- the Charts commands

#[test]
fn c2_the_charts_group_has_its_five_commands_with_reasons_when_they_cannot_run() {
    let ids: Vec<CommandId> = in_group(CommandGroup::Charts).map(|c| c.id).collect();
    assert_eq!(ids, vec![CommandId::ChartNew, CommandId::ChartThis, CommandId::ChartOpenSaved, CommandId::ChartBook, CommandId::Dashboard]);
    let titles: Vec<&str> = in_group(CommandGroup::Charts).map(|c| c.title).collect();
    assert_eq!(titles, ["New chart\u{2026}", "Chart this result", "Open saved chart\u{2026}", "Chart book (HTML)\u{2026}", "Dashboard\u{2026}"]);
    let st = |tabs: usize, result_tab: bool| CommandState { tabs, result_tab, ..Default::default() };
    let why = |id: CommandId, s: CommandState| unavailable_reason(command(id).unwrap(), s);
    assert!(why(CommandId::ChartNew, st(0, false)).is_some() && why(CommandId::ChartNew, st(1, false)).is_none());
    // "Chart this result" is for tool result tabs
    assert!(why(CommandId::ChartThis, st(1, false)).unwrap().contains("result tab") && why(CommandId::ChartThis, st(1, true)).is_none());
    assert!(why(CommandId::ChartOpenSaved, st(0, false)).is_some() && why(CommandId::ChartOpenSaved, st(2, false)).is_none());
    // the chart book (C3) and the HTML export of one chart need an open tab
    assert!(why(CommandId::ChartBook, st(0, false)).is_some() && why(CommandId::ChartBook, st(3, true)).is_none());
    assert!(why(CommandId::ExportChartHtml, st(0, false)).is_some() && why(CommandId::ExportChartHtml, st(1, false)).is_none());
    assert!(why(CommandId::ExportChartXlsx, st(0, false)).is_some() && why(CommandId::ExportChartXlsx, st(1, false)).is_none());
    // Ctrl+K finds them by what they do
    let entries: Vec<micro_parquet::commands::Entry> = micro_parquet::commands::COMMANDS.iter().map(|c| micro_parquet::commands::Entry { label: c.title, words: c.words, recent: None }).collect();
    let top = |q: &str| micro_parquet::commands::COMMANDS[micro_parquet::commands::rank(q, &entries)[0]].id;
    assert_eq!(top("new chart"), CommandId::ChartNew);
    assert_eq!(top("open saved chart"), CommandId::ChartOpenSaved);
    assert_eq!(top("chart book"), CommandId::ChartBook);
    assert_eq!(top("chart this"), CommandId::ChartThis);
}
