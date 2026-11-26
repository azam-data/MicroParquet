//! Phase 14 A2: the assistant never calculates; and a model's wording may not hold a number of its own.

use micro_parquet::assistant::*;

#[test]
fn a2_a_a_request_for_a_figure_is_not_answered_with_one() {
    for q in [
        "calculate the NIM for 4,500 Cr of advances at 9.5%",
        "compute duration of a 5 year bond at 9%",
        "what is the provision on 120 Cr classified as bad loan",
        "how much is the LCR if HQLA is 2000 crore",
        "work out the gap for 30 days",
        "calculate NIM",
    ] {
        assert_eq!(intent(q), Intent::Calculate, "{q}");
        let a = embedded().answer(q);
        assert_eq!(a.kind, AnswerKind::NotCalculated, "{q}");
        assert!(a.text.starts_with("I do not calculate numbers"), "{}", a.text);
        // the reply holds no figure of its own
        let labels: Vec<&str> = a.sources.iter().map(|s| s.label.as_str()).collect();
        assert!(unsupported_numbers(&a.text, &labels).is_empty(), "{}", a.text);
    }
}

#[test]
fn a2_b_a_calculate_request_points_to_the_tool() {
    let a = embedded().answer("calculate the liquidity gap ladder for my loans");
    assert_eq!(a.kind, AnswerKind::NotCalculated);
    assert!(!a.sources.is_empty(), "the tool's note should be listed");
    assert!(a.text.contains(&a.sources[0].label));
}

#[test]
fn a2_c_how_a_figure_is_made_is_a_question_the_guide_answers() {
    for q in [
        "how is the NIM calculated",
        "what is the formula for duration",
        "how does the provision get calculated",
        "explain the LCR",
        "what is the gap ladder",
        "how do I calculate NIM in the app",
    ] {
        assert_eq!(intent(q), Intent::Explain, "{q}");
    }
}

#[test]
fn a2_d_numbers_the_sources_do_not_hold_are_found() {
    let sources = ["LCR is HQLA divided by net outflows over 30 days, at least 100%."];
    // 30 and 100 come from the source, 42 does not
    assert_eq!(unsupported_numbers("The LCR needs 100% over 30 days.", &sources), Vec::<String>::new());
    assert_eq!(unsupported_numbers("Your LCR is 42%.", &sources), vec!["42".to_string()]);
    // list markers and source markers are not numbers of the answer
    assert!(unsupported_numbers("1. Open the tab\n2. Click Run\n[1] used", &sources).is_empty());
    // 4,500 and 4500 are the same number; 9.5 is not 95
    assert!(unsupported_numbers("Balance 4500", &["it was 4,500"]).is_empty());
    assert_eq!(unsupported_numbers("rate 9.5", &["rate 95"]), vec!["9.5".to_string()]);
    // a number the user typed in the question may be repeated
    assert!(unsupported_numbers("For 30 days use the 30-day bucket.", &["a 30 days question"]).is_empty());
}

#[test]
fn a2_e_a_model_reply_with_a_new_number_is_replaced_by_the_guide_text() {
    let g = embedded();
    let q = "how do I run a liquidity gap ladder";
    let (_, hits) = g.search(q);
    let secs: Vec<usize> = hits.iter().map(|h| h.section).collect();
    let bad = vet_model_reply(g, q, "Open the tab and the gap is 12345 Cr.", &secs);
    assert_eq!(bad.kind, AnswerKind::Guide);
    assert!(bad.note.as_deref().unwrap_or("").contains("12345"), "{:?}", bad.note);
    let good = vet_model_reply(g, q, "Open your loan tab, choose the gap ladder and run it. [1]", &secs);
    assert_eq!(good.kind, AnswerKind::Model);
    assert!(good.note.is_none());
    assert_eq!(good.sources.len(), secs.len().min(3));
    // an empty reply falls back too
    assert_eq!(vet_model_reply(g, q, "  ", &secs).kind, AnswerKind::Guide);
}

#[test]
fn a2_f_a_general_explanation_may_hold_no_number_and_is_labelled() {
    let g = embedded();
    let ok = vet_model_reply(g, "what is convexity", "Convexity describes how the price-yield curve bends.", &[]);
    assert_eq!(ok.kind, AnswerKind::ModelGeneral);
    assert!(ok.sources.is_empty());
    let bad = vet_model_reply(g, "what is convexity", "Convexity is about 85 for a bond.", &[]);
    assert_ne!(bad.kind, AnswerKind::ModelGeneral);
}

#[test]
fn a2_g_thinking_text_is_removed() {
    assert_eq!(strip_thinking("<think>hmm 12</think>The answer."), "The answer.");
    assert_eq!(strip_thinking("A<think>unfinished"), "A");
}
