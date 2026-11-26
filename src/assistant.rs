//! Phase 14: the help assistant's brain that needs no model: a search of the guide book (`docs/guide/*.md`, compiled into the
//! program), the rule that the assistant never calculates, and the check that a model's wording holds no number of its own.
//!
//! The search splits every note into its sections and ranks them with BM25 (a long-standing word-weighting method), with the
//! words of a heading and of the note's title counting three times. It needs no file, no network and no model. Every answer
//! names the notes and sections it came from. A question the guide does not cover gets "not covered", never a guess.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/guide_embedded.rs"));

/// The most results a search gives.
pub const MAX_HITS: usize = 4;
/// At most this many sections of one note in the results.
const PER_NOTE: usize = 2;
/// The characters of a section shown as the answer's excerpt (cut at a line).
pub const EXCERPT_CHARS: usize = 900;
/// The characters of sources handed to a model, and of one section among them.
pub const MODEL_SOURCE_CHARS: usize = 6000;
pub const MODEL_SECTION_CHARS: usize = 2200;
/// A query word found in more than this share of the sections says little (it is left out of the coverage check).
const GENERIC_SHARE: f32 = 0.30;
/// Less than this share of the informative words found in a section: no match.
const MIN_COVERAGE: f32 = 0.5;

/// One section of one note.
#[derive(Clone, Debug)]
pub struct Section {
    /// The note's file name (`gap-ladder.md`).
    pub file: String,
    /// The note's title (its first line).
    pub doc_title: String,
    /// The section's heading without its number ("Step by step"); empty for the lead before the first heading.
    pub heading: String,
    /// The section's text as written.
    pub body: String,
}

impl Section {
    /// "Gap ladder ▸ Step by step", or just the title for a lead.
    pub fn label(&self) -> String {
        if self.heading.is_empty() {
            self.doc_title.clone()
        } else {
            format!("{} \u{25b8} {}", self.doc_title, self.heading)
        }
    }
}

/// One search result.
#[derive(Clone, Debug)]
pub struct Hit {
    /// The section's index in the guide.
    pub section: usize,
    pub score: f32,
}

/// What a search found, with the reason when it found nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchOutcome {
    Found(Vec<usize>),
    /// The question has no word the guide can be searched by ("how do I?").
    NoSearchWords,
    /// None of the sections holds enough of the question's words.
    NotCovered,
}

/// The guide book, split into sections and indexed.
pub struct Guide {
    sections: Vec<Section>,
    tf: Vec<HashMap<String, f32>>,
    lens: Vec<f32>,
    df: HashMap<String, u32>,
    avg_len: f32,
}

/// The guide compiled into the program.
pub fn embedded() -> &'static Guide {
    static G: OnceLock<Guide> = OnceLock::new();
    G.get_or_init(|| Guide::from_files(GUIDE_FILES))
}

const STOP: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "to", "of", "in", "on", "at", "for", "and", "or", "it", "its", "this", "that",
    "these", "those", "with", "as", "by", "from", "do", "does", "did", "i", "me", "my", "we", "you", "your", "can", "could", "should",
    "would", "will", "how", "what", "which", "when", "where", "who", "why", "there", "not", "no", "if", "then", "so", "than", "but",
    "any", "about", "into", "get", "has", "have", "had", "use", "using", "want", "need", "please", "tell", "show", "give", "make",
    "one", "also", "just", "up", "out", "all", "some", "s",
];

/// Words a banker types that the notes spell another way: (typed, also search).
const SYNONYMS: &[(&str, &[&str])] = &[
    ("npl", &["classified", "overdue"]),
    ("nplm", &["classified", "overdue"]),
    ("bad", &["classified", "defaulted"]),
    ("default", &["classified", "overdue"]),
    ("dpd", &["overdue", "days"]),
    ("provisioning", &["provision"]),
    ("liquidity", &["lcr", "nsfr", "gap"]),
    ("irrbb", &["interest", "rate", "shock", "eve"]),
    ("eve", &["duration", "dv01"]),
    ("repricing", &["gap", "bucket"]),
    ("maturity", &["bucket", "gap", "ladder"]),
    ("alm", &["gap", "ladder", "liquidity"]),
    ("ftp", &["funds", "transfer", "pricing", "curve"]),
    ("nii", &["interest", "income"]),
    ("excel", &["xlsx"]),
    ("spreadsheet", &["xlsx"]),
    ("slides", &["pptx", "powerpoint"]),
    ("powerpoint", &["pptx"]),
    ("ppt", &["pptx", "powerpoint"]),
    ("lookup", &["join", "link"]),
    ("vlookup", &["join", "link"]),
    ("merge", &["join", "stack"]),
    ("append", &["stack"]),
    ("combine", &["stack", "join"]),
    ("colour", &["color"]),
    ("color", &["colour"]),
    ("graph", &["chart"]),
    ("plot", &["chart"]),
    ("duplicate", &["key", "check"]),
    ("sign", &["dr", "cr"]),
    ("negative", &["sign"]),
    ("fiscal", &["year", "calendar"]),
    ("mtd", &["time", "intelligence"]),
    ("ytd", &["time", "intelligence"]),
    ("qtd", &["time", "intelligence"]),
];

/// A word as the index holds it: lower case, plural and "-ing"/"-ed" endings cut. Both the notes and the question go through it.
pub fn stem(w: &str) -> String {
    let mut s = w.to_lowercase();
    if s.len() > 4 && s.ends_with("ies") {
        s.truncate(s.len() - 3);
        s.push('y');
    } else if s.len() > 5 && s.ends_with("ing") {
        s.truncate(s.len() - 3);
    } else if s.len() > 4 && s.ends_with("ed") {
        s.truncate(s.len() - 2);
    } else if s.len() > 3 && s.ends_with('s') && !s.ends_with("ss") {
        s.pop();
    }
    s
}

/// The searchable words of a text: letters and digits, lower case, stemmed, without the filler words.
pub fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            let w = cur.to_lowercase();
            if !STOP.contains(&w.as_str()) {
                out.push(stem(&w));
            }
            cur.clear();
        }
    };
    for c in text.chars() {
        if c.is_alphanumeric() {
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Markdown links and pictures reduced to their words, so a file name or a path is not a search word.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // [words](target) -> words ; ![alt](target) -> alt
        if chars[i] == '[' {
            if let Some(close) = chars[i..].iter().position(|c| *c == ']') {
                let j = i + close;
                if chars.get(j + 1) == Some(&'(') {
                    if let Some(end) = chars[j..].iter().position(|c| *c == ')') {
                        out.extend(&chars[i + 1..j]);
                        i = j + end + 1;
                        continue;
                    }
                }
            }
        }
        if chars[i] == '!' && chars.get(i + 1) == Some(&'[') {
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// "5. Step by step" -> "Step by step"; "7a. Labels" -> "Labels".
fn heading_text(h: &str) -> String {
    let h = h.trim();
    let digits = h.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &h[digits..];
        let rest = rest.strip_prefix(|c: char| c.is_ascii_lowercase()).unwrap_or(rest);
        if let Some(r) = rest.strip_prefix('.') {
            return r.trim().to_string();
        }
    }
    h.to_string()
}

/// A note split at its `##` headings (a `#` or `##` inside a code fence is not a heading). The text before the first `##` is the lead.
pub fn split_note(file: &str, text: &str) -> Vec<Section> {
    let mut title = String::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut heading = String::new();
    let mut body = String::new();
    let mut fenced = false;
    let mut flush = |heading: &mut String, body: &mut String, title: &str, sections: &mut Vec<Section>| {
        if !body.trim().is_empty() {
            sections.push(Section { file: file.to_string(), doc_title: title.to_string(), heading: heading.clone(), body: body.trim().to_string() });
        }
        body.clear();
    };
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced {
            if let Some(t) = line.strip_prefix("# ") {
                if title.is_empty() {
                    title = t.trim().to_string();
                    continue;
                }
            }
            if let Some(h) = line.strip_prefix("## ") {
                flush(&mut heading, &mut body, &title, &mut sections);
                heading = heading_text(h);
                continue;
            }
        }
        body.push_str(line);
        body.push('\n');
    }
    flush(&mut heading, &mut body, &title, &mut sections);
    // a note with no title line is named after its file
    if title.is_empty() {
        let t = file.trim_end_matches(".md").replace('-', " ");
        for s in &mut sections {
            s.doc_title = t.clone();
        }
    }
    sections
}

impl Guide {
    /// Indexes the given notes (file name, text). The README (a list of links) is left out: it would match everything.
    pub fn from_files(files: &[(&str, &str)]) -> Guide {
        let mut sections = Vec::new();
        for (name, text) in files {
            if name.eq_ignore_ascii_case("README.md") {
                continue;
            }
            sections.extend(split_note(name, text));
        }
        let mut tf = Vec::with_capacity(sections.len());
        let mut lens = Vec::with_capacity(sections.len());
        let mut df: HashMap<String, u32> = HashMap::new();
        for s in &sections {
            let mut m: HashMap<String, f32> = HashMap::new();
            let mut len = 0.0;
            for t in tokens(&plain(&s.body)) {
                *m.entry(t).or_insert(0.0) += 1.0;
                len += 1.0;
            }
            // the heading and the note's title count three times
            for t in tokens(&s.heading).into_iter().chain(tokens(&s.doc_title)) {
                *m.entry(t).or_insert(0.0) += 3.0;
                len += 3.0;
            }
            for k in m.keys() {
                *df.entry(k.clone()).or_insert(0) += 1;
            }
            tf.push(m);
            lens.push(len);
        }
        let avg_len = if lens.is_empty() { 1.0 } else { (lens.iter().sum::<f32>() / lens.len() as f32).max(1.0) };
        Guide { sections, tf, lens, df, avg_len }
    }

    pub fn len(&self) -> usize {
        self.sections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    pub fn section(&self, i: usize) -> Option<&Section> {
        self.sections.get(i)
    }

    /// The indexes of every section of a note, in order.
    pub fn note_sections(&self, file: &str) -> Vec<usize> {
        self.sections.iter().enumerate().filter(|(_, s)| s.file == file).map(|(i, _)| i).collect()
    }

    fn idf(&self, term: &str) -> f32 {
        let n = self.sections.len() as f32;
        let df = self.df.get(term).copied().unwrap_or(0) as f32;
        (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
    }

    /// The heading kinds the question's shape points at: "how do I" wants the steps, "what is" the first section, "error" the messages.
    fn intent_headings(query: &str) -> &'static [&'static str] {
        let q = query.to_lowercase();
        let has = |w: &str| q.split(|c: char| !c.is_alphanumeric()).any(|t| t == w);
        if has("error") || has("message") || has("refuses") || has("refused") || has("fails") || has("failed") || has("stops") || has("stopped") || has("wrong") {
            &["common messages and fixes", "what it refuses and why"]
        } else if q.contains("how do") || q.contains("how to") || q.contains("how can") || q.contains("steps") || has("step") || q.starts_with("how ") {
            &["step by step", "how to use it"]
        } else if q.contains("what is") || q.contains("what does") || q.contains("what are") || q.contains("what's") || has("purpose") || has("meaning") {
            &["what it does"]
        } else if has("columns") || q.contains("what data") || q.contains("which column") {
            &["columns you need", "before you start"]
        } else if has("limit") || has("limits") || has("assumption") || has("assumptions") {
            &["limits and assumptions"]
        } else {
            &[]
        }
    }

    /// The best sections for a question, best first: at most four, at most two from one note.
    pub fn search(&self, query: &str) -> (SearchOutcome, Vec<Hit>) {
        let typed: Vec<String> = {
            let mut seen = HashSet::new();
            tokens(query).into_iter().filter(|t| seen.insert(t.clone())).collect()
        };
        if typed.is_empty() {
            return (SearchOutcome::NoSearchWords, Vec::new());
        }
        // synonyms add words that count for less
        let mut extra: Vec<String> = Vec::new();
        for t in &typed {
            for (k, v) in SYNONYMS {
                if stem(k) == *t {
                    extra.extend(v.iter().map(|w| stem(w)));
                }
            }
        }
        extra.retain(|e| !typed.contains(e));
        extra.sort();
        extra.dedup();
        // informative words: the ones that tell sections apart; a word the guide has never seen counts against the match
        let n = self.sections.len().max(1) as f32;
        let informative: Vec<&String> = typed.iter().filter(|t| (self.df.get(*t).copied().unwrap_or(0) as f32) / n <= GENERIC_SHARE).collect();
        if informative.is_empty() {
            return (SearchOutcome::NoSearchWords, Vec::new());
        }
        let wanted_headings = Self::intent_headings(query);
        let k1 = 1.2f32;
        let b = 0.75f32;
        let mut scored: Vec<Hit> = Vec::new();
        for (i, tfm) in self.tf.iter().enumerate() {
            let dl = self.lens[i];
            let norm = k1 * (1.0 - b + b * dl / self.avg_len);
            let mut score = 0.0f32;
            for t in typed.iter().map(|t| (t, 1.0f32)).chain(extra.iter().map(|t| (t, 0.5f32))) {
                if let Some(f) = tfm.get(t.0) {
                    score += t.1 * self.idf(t.0) * f * (k1 + 1.0) / (f + norm);
                }
            }
            if score <= 0.0 {
                continue;
            }
            let found = informative.iter().filter(|t| tfm.contains_key(**t) || extra.iter().any(|e| tfm.contains_key(e) && self.synonym_of(t, e))).count();
            let coverage = found as f32 / informative.len() as f32;
            if coverage < MIN_COVERAGE {
                continue;
            }
            score *= 0.5 + 0.5 * coverage;
            let h = self.sections[i].heading.to_lowercase();
            if wanted_headings.iter().any(|w| h == *w) {
                score *= 1.25;
            }
            scored.push(Hit { section: i, score });
        }
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(a.section.cmp(&b.section)));
        let mut hits: Vec<Hit> = Vec::new();
        let mut per_note: HashMap<&str, usize> = HashMap::new();
        for h in scored {
            let file = self.sections[h.section].file.as_str();
            let c = per_note.entry(file).or_insert(0);
            if *c >= PER_NOTE {
                continue;
            }
            *c += 1;
            hits.push(h);
            if hits.len() == MAX_HITS {
                break;
            }
        }
        if hits.is_empty() {
            (SearchOutcome::NotCovered, hits)
        } else {
            (SearchOutcome::Found(hits.iter().map(|h| h.section).collect()), hits)
        }
    }

    fn synonym_of(&self, typed: &str, extra: &str) -> bool {
        SYNONYMS.iter().any(|(k, v)| stem(k) == typed && v.iter().any(|w| stem(w) == extra))
    }
}

// ------------------------------------------------------------------------------------------ the never-calculate rule

/// What the question asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    /// "How do I...", "what is...", "which column...": the guide answers it.
    Explain,
    /// "Calculate the NIM of 4,500 Cr at 9.5%": a figure is wanted, and the app's own function must produce it.
    Calculate,
}

/// True when the text has a figure in it: a number of two or more digits or a decimal, or a number with a unit (5 cr, 9%, 100bps).
fn has_figure(q: &str) -> bool {
    let lower = q.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '%' && c != ',');
        let digits: String = w.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        if digits.len() >= 2 || w.contains('.') || w.contains('%') || w.contains(',') {
            return true;
        }
        // "5 cr", "9 percent", "50 bps"
        if let Some(next) = words.get(i + 1) {
            let next = next.trim_matches(|c: char| !c.is_alphanumeric());
            if ["cr", "crore", "crores", "lakh", "lac", "bdt", "tk", "taka", "percent", "bps", "bp", "million", "mn", "days", "day", "months", "years", "year"].contains(&next) {
                return true;
            }
        }
        if w.ends_with("cr") || w.ends_with("bps") || w.ends_with("bp") || w.ends_with('%') {
            return true;
        }
    }
    false
}

/// Whether the question wants a number made. A figure in the question together with a "work it out" word does; so does a
/// plain "calculate / compute / work out" request. "How is NIM calculated?" and "formula for duration" do not: they ask how.
pub fn intent(query: &str) -> Intent {
    let q = query.to_lowercase();
    let q = q.trim();
    let words: Vec<&str> = q.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |w: &str| words.contains(&w);
    let explains = q.contains("how is") || q.contains("how are") || q.contains("how does") || q.contains("how do") || q.contains("how to") || has("formula") || has("explain") || has("definition") || has("meaning") || has("why") || q.contains("what is the") && !has_figure(q) && !q.contains("what is the value");
    let work_out = has("calculate") || has("compute") || has("calc") || q.contains("work out") || q.contains("how much") || has("convert") || q.contains("add up") || q.contains("what will") || has("estimate") || q.contains("what would") || q.contains("find the value");
    let figure = has_figure(q);
    let leads_with_verb = ["calculate", "compute", "calc", "work", "convert", "estimate", "add", "sum"].contains(&words.first().copied().unwrap_or(""));
    if figure && (work_out || q.starts_with("what is") || q.starts_with("what's") || q.contains("?")) && !(explains && q.contains("formula")) {
        return Intent::Calculate;
    }
    if leads_with_verb && !explains {
        return Intent::Calculate;
    }
    if work_out && !explains {
        return Intent::Calculate;
    }
    Intent::Explain
}

// ------------------------------------------------------------------------------------------ the answer

/// A section an answer used.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub section: usize,
    pub file: String,
    /// "Gap ladder ▸ Step by step".
    pub label: String,
}

/// How the answer was made.
#[derive(Clone, Debug, PartialEq)]
pub enum AnswerKind {
    /// The text of the best section, from the guide book.
    Guide,
    /// A local model wrote it from the sources.
    Model,
    /// A local model explained a concept the guide does not hold (labelled so).
    ModelGeneral,
    /// The question wants a figure: the assistant points to the app's function.
    NotCalculated,
    /// The guide does not cover the question.
    NotCovered,
}

#[derive(Clone, Debug)]
pub struct Answer {
    pub kind: AnswerKind,
    pub text: String,
    /// The sections used, best first (empty when none).
    pub sources: Vec<Source>,
    /// A line the reader should see with the answer (a rejected model wording, say).
    pub note: Option<String>,
}

/// The first lines of a section, cut at a line, at most `max` characters. Says so when it cut.
pub fn excerpt(body: &str, max: usize) -> String {
    let body = plain(body);
    let mut out = String::new();
    let mut cut = false;
    for line in body.lines() {
        if out.chars().count() + line.chars().count() + 1 > max && !out.trim().is_empty() {
            cut = true;
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    let mut out = out.trim().to_string();
    if cut {
        out.push_str(" \u{2026}");
    }
    out
}

impl Guide {
    fn source(&self, i: usize) -> Option<Source> {
        let s = self.sections.get(i)?;
        Some(Source { section: i, file: s.file.clone(), label: s.label() })
    }

    /// The answer to a question from the guide alone: the best section's text, and the other sections that match. The words come
    /// from the guide book; none is made up.
    pub fn answer(&self, query: &str) -> Answer {
        let calc = intent(query) == Intent::Calculate;
        let (outcome, hits) = self.search(query);
        let sources: Vec<Source> = hits.iter().filter_map(|h| self.source(h.section)).collect();
        if calc {
            let mut text = String::from("I do not calculate numbers, so that no figure in your work comes from a guess. The app calculates it with its own function, from your data.");
            if let Some(first) = sources.first() {
                text.push_str(&format!("\n\nThe tool to use is described in \"{}\". Open it below, or search the tool with Ctrl+K.", first.label));
            } else {
                text.push_str("\n\nSearch the tool with Ctrl+K, or ask me how the calculation is done (for example \"how is the NIM calculated\").");
            }
            return Answer { kind: AnswerKind::NotCalculated, text, sources, note: None };
        }
        match outcome {
            SearchOutcome::Found(_) => {
                let best = hits[0].section;
                let sec = &self.sections[best];
                let text = format!("{}\n\nFrom: {}", excerpt(&sec.body, EXCERPT_CHARS), sec.label());
                Answer { kind: AnswerKind::Guide, text, sources, note: None }
            }
            SearchOutcome::NoSearchWords => Answer {
                kind: AnswerKind::NotCovered,
                text: "I need a few more words to search the guide book: name the tool or the task (for example \"liquidity gap ladder\" or \"join two tabs\").".to_string(),
                sources: Vec::new(),
                note: None,
            },
            SearchOutcome::NotCovered => Answer {
                kind: AnswerKind::NotCovered,
                text: "The guide book does not cover this. Try other words, or search the tool with Ctrl+K.".to_string(),
                sources: Vec::new(),
                note: None,
            },
        }
    }

    /// The sources text for a model: up to three sections, each cut, numbered [1], [2], [3].
    pub fn model_sources(&self, sections: &[usize]) -> String {
        let mut out = String::new();
        let mut used = 0;
        for (n, i) in sections.iter().take(3).enumerate() {
            let Some(s) = self.sections.get(*i) else { continue };
            let text = excerpt(&s.body, MODEL_SECTION_CHARS);
            if used + text.len() > MODEL_SOURCE_CHARS && n > 0 {
                break;
            }
            used += text.len();
            out.push_str(&format!("[{}] {}\n{}\n\n", n + 1, s.label(), text));
        }
        out
    }
}

/// The rules a model is given, for an answer that must come from the sources.
pub const SYSTEM_GROUNDED: &str = "You are the help assistant inside MicroParquet, a desktop program that bank treasury, ALM and finance staff in Bangladesh use. \
Answer only from the SOURCES in the user's message. If they do not hold the answer, say exactly: The guide book does not cover this. \
Never calculate, estimate or invent a number, rate, date or amount, and never repeat a number that is not in the SOURCES or in the question. \
If the user wants a figure worked out, say that the program's own function does it and name the function from the SOURCES. \
Use plain English, at most 150 words, numbered steps for a procedure, and name the sources you used as [1], [2].";

/// The rules for a concept the guide does not hold.
pub const SYSTEM_GENERAL: &str = "You are the help assistant inside MicroParquet, a desktop program that bank treasury, ALM and finance staff in Bangladesh use. \
Explain the banking, ALM or financial-mathematics idea in plain words, at most 120 words. \
Do not write any number, rate, date or amount of your own, and do not calculate. Do not describe how MicroParquet works: you do not have its guide here. \
If you are not sure, say so.";

/// The user message for a grounded answer.
pub fn grounded_user_message(sources: &str, question: &str) -> String {
    format!("SOURCES\n{}\nQUESTION\n{}", sources, question.trim())
}

/// Numbers in a model's answer that are in neither the allowed texts nor a list marker. A number of the answer must be one the
/// guide or the question already holds, or it is not shown.
pub fn unsupported_numbers(answer: &str, allowed: &[&str]) -> Vec<String> {
    fn numbers(text: &str, skip_markers: bool) -> Vec<String> {
        let mut out = Vec::new();
        for line in text.lines() {
            let mut s = line.trim_start();
            if skip_markers {
                // "1. Step", "2) Step", "[1] source", "- 3"
                s = s.trim_start_matches(|c: char| c == '-' || c == '*' || c == '#' || c == ' ');
                let d = s.chars().take_while(|c| c.is_ascii_digit()).count();
                if d > 0 && s[d..].starts_with(['.', ')']) {
                    s = &s[d + 1..];
                }
            }
            let mut cur = String::new();
            let chars: Vec<char> = s.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c.is_ascii_digit() {
                    cur.push(c);
                } else if (c == '.' || c == ',') && !cur.is_empty() && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
                    // a thousands or decimal mark inside a number is dropped, so 4,500 and 4500 match
                    if c == '.' {
                        cur.push('.');
                    }
                } else {
                    if !cur.is_empty() {
                        // "[1]" source markers
                        let bracketed = i >= cur.len() + 1 && chars.get(i - cur.len() - 1) == Some(&'[') && c == ']';
                        if !bracketed {
                            out.push(std::mem::take(&mut cur));
                        }
                        cur.clear();
                    }
                }
                i += 1;
            }
            if !cur.is_empty() {
                out.push(cur);
            }
        }
        out
    }
    let allowed_set: HashSet<String> = allowed.iter().flat_map(|t| numbers(t, false)).collect();
    let mut bad: Vec<String> = numbers(answer, true).into_iter().filter(|n| !allowed_set.contains(n)).collect();
    bad.sort();
    bad.dedup();
    bad
}

/// Takes a model's reply and either keeps it (with its sources) or falls back to the guide's own text with a note saying why.
/// `grounded` is false for a general explanation, where no number at all is allowed (apart from list markers).
pub fn vet_model_reply(guide: &Guide, query: &str, reply: &str, grounded_sections: &[usize]) -> Answer {
    let reply = strip_thinking(reply);
    let sources: Vec<Source> = grounded_sections.iter().take(3).filter_map(|i| guide.source(*i)).collect();
    let allowed_owned: Vec<String> = grounded_sections.iter().take(3).filter_map(|i| guide.section(*i)).map(|s| s.body.clone()).collect();
    let mut allowed: Vec<&str> = allowed_owned.iter().map(|s| s.as_str()).collect();
    allowed.push(query);
    let bad = unsupported_numbers(&reply, &allowed);
    if reply.trim().is_empty() {
        let mut a = guide.answer(query);
        a.note = Some("The model gave no answer, so this is the guide book's own text.".to_string());
        return a;
    }
    if !bad.is_empty() {
        let mut a = guide.answer(query);
        a.note = Some(format!("The model's wording had numbers that are not in the guide book ({}), so it is not shown. This is the guide book's own text.", bad.join(", ")));
        return a;
    }
    let kind = if sources.is_empty() { AnswerKind::ModelGeneral } else { AnswerKind::Model };
    Answer { kind, text: reply.trim().to_string(), sources, note: None }
}

/// Some models write their reasoning between <think> tags first.
pub fn strip_thinking(reply: &str) -> String {
    let mut s = reply.to_string();
    while let Some(a) = s.find("<think>") {
        match s[a..].find("</think>") {
            Some(b) => s.replace_range(a..a + b + "</think>".len(), ""),
            None => {
                s.truncate(a);
            }
        }
    }
    s.trim().to_string()
}

/// A section's text for the reading pane: links reduced to their words, `**` and back-ticks taken off, table separator lines dropped.
pub fn display_text(body: &str) -> String {
    let mut out = String::new();
    for line in plain(body).lines() {
        let t = line.trim();
        if t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }
        out.push_str(&line.replace("**", "").replace('`', ""));
        out.push('\n');
    }
    out.trim_end().to_string()
}
