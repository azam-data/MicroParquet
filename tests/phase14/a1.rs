//! Phase 14 A1: the guide-book search finds the right note and says which section it used.

use micro_parquet::assistant::*;

fn files(q: &str) -> Vec<String> {
    let g = embedded();
    g.search(q).1.iter().map(|h| g.section(h.section).expect("section").file.clone()).collect()
}

#[test]
fn a1_a_the_guide_is_compiled_in_and_split_into_sections() {
    let g = embedded();
    assert!(g.len() > 500, "{} sections", g.len());
    // the README is a list of links and is not searched
    assert!(g.note_sections("README.md").is_empty());
    let s = g.note_sections("gap-ladder.md");
    assert!(s.len() >= 8, "{s:?}");
    let first = g.section(s[0]).expect("lead");
    assert_eq!(first.doc_title.is_empty(), false);
    // numbers are taken off the headings
    assert!(s.iter().any(|i| g.section(*i).expect("s").heading == "Step by step"));
}

#[test]
fn a1_b_questions_a_banker_would_ask_reach_their_note() {
    for (q, file) in [
        ("how do I run a liquidity gap ladder", "gap-ladder.md"),
        ("what does the CL-1 summary do", "cl1-summary.md"),
        ("how do I join two tabs by a key", "join.md"),
        ("how is expected credit loss stage 2 worked out", "ifrs9-ecl.md"),
        ("export dashboard to powerpoint", "export-pptx.md"),
        ("duplicate keys in a lookup table", "key-check.md"),
        ("what is DV01 and EVE", "duration-dv01-eve.md"),
        ("change the first day of the week and fiscal year", "calendar-settings.md"),
        ("forecast deposits for the next six months", "forecasting.md"),
        ("reverse the sign of advances that arrive negative", "schema-keep-reverse-sign.md"),
        ("top movers each month", "top-movers.md"),
        ("save the filters of a dashboard page as a view", "saved-views.md"),
        ("what if deposits grow with a scenario", "what-if-scenarios.md"),
        ("actual against budget achieved", "plan-vs-actual.md"),
        ("rolling 12 month window", "rolling-measures.md"),
        ("volume rate and mix bridge of interest", "variance-bridge.md"),
        ("add a note to a chart category", "chart-notes.md"),
        ("green amber red target on a kpi card", "target-bands.md"),
    ] {
        let got = files(q);
        assert!(got.contains(&file.to_string()), "{q:?} gave {got:?}, wanted {file}");
        assert_eq!(got[0], file, "{q:?} should put {file} first, got {got:?}");
    }
}

#[test]
fn a1_c_a_how_question_prefers_the_steps_and_a_what_question_the_first_section() {
    let g = embedded();
    let (_, h) = g.search("how do I use the gap ladder");
    assert_eq!(g.section(h[0].section).expect("s").heading, "Step by step", "{:?}", h.iter().map(|x| g.section(x.section).expect("s").label()).collect::<Vec<_>>());
    let (_, h) = g.search("what is the gap ladder");
    assert_eq!(g.section(h[0].section).expect("s").heading, "What it does");
}

#[test]
fn a1_d_what_the_guide_does_not_cover_is_said_not_guessed() {
    let g = embedded();
    for q in ["recipe for hilsa curry", "xyzzy plugh quux", "weather in dhaka tomorrow"] {
        let a = g.answer(q);
        assert_eq!(a.kind, AnswerKind::NotCovered, "{q}: {:?}", a.text);
        assert!(a.sources.is_empty());
    }
    // no search word at all
    assert_eq!(g.search("how do I?").0, SearchOutcome::NoSearchWords);
}

#[test]
fn a1_e_an_answer_names_its_sections_and_quotes_the_guide() {
    let g = embedded();
    let a = g.answer("how do I run a liquidity gap ladder");
    assert_eq!(a.kind, AnswerKind::Guide);
    assert!(!a.sources.is_empty() && a.sources.len() <= MAX_HITS);
    assert!(a.text.contains("From: "), "{}", a.text);
    assert!(a.text.contains(&a.sources[0].label), "{}", a.text);
    // at most two sections of one note
    for f in ["gap-ladder.md"] {
        assert!(a.sources.iter().filter(|s| s.file == f).count() <= 2);
    }
    // every line of the excerpt is a line of the section: nothing is made up
    let sec = g.section(a.sources[0].section).expect("s");
    let body = sec.body.replace("**", "");
    let first_line = a.text.lines().next().expect("line").replace("**", "");
    assert!(body.contains(first_line.trim()), "excerpt line not in the section: {first_line}");
}

#[test]
fn a1_f_the_excerpt_is_cut_at_a_line_and_says_so() {
    let long = (0..100).map(|i| format!("line number {i} of the section")).collect::<Vec<_>>().join("\n");
    let e = excerpt(&long, 200);
    assert!(e.ends_with('\u{2026}'));
    assert!(e.lines().all(|l| l.starts_with("line number") || l.ends_with('\u{2026}')));
    assert!(!excerpt("short", 200).contains('\u{2026}'));
}

#[test]
fn a1_g_headings_inside_a_code_fence_do_not_split_a_note() {
    let text = "# Note\n\nLead words.\n\n## 1. One\n\n```\n## not a heading\n```\n\n## 2. Two\n\nSecond.\n";
    let s = split_note("n.md", text);
    assert_eq!(s.len(), 3);
    assert_eq!(s[1].heading, "One");
    assert!(s[1].body.contains("## not a heading"));
    assert_eq!(s[2].heading, "Two");
    assert_eq!(s[0].heading, "");
    assert_eq!(s[0].doc_title, "Note");
}

#[test]
fn a1_h_search_is_the_same_each_time() {
    let a = files("loan overdue days classification");
    let b = files("loan overdue days classification");
    assert_eq!(a, b);
    assert!(a.iter().any(|f| f == "classify-by-overdue.md"), "{a:?}");
}
