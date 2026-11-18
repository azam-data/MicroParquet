//! Phase 5 A4: the shipped Bangladesh Bank rule files. The source copies are in the repository under
//! `assets/rules/` and built in with `include_str!`; a file is written to `portable_data/rules/<kind>/`
//! only when it is missing, and an existing file is never overwritten.

use crate::common::*;
use chrono::NaiveDate;
use micro_parquet::rules::*;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("a test date")
}

fn assets_path(kind: &str, date: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("rules").join(kind).join(format!("{date}.json"))
}

#[test]
fn a4_every_shipped_rule_file_has_its_source_copy_under_assets_rules_and_is_built_in_from_it() {
    assert!(SHIPPED_RULES.len() >= 2);
    for (kind, (date, text)) in SHIPPED_RULES {
        let on_disk = std::fs::read_to_string(assets_path(kind, date)).unwrap_or_else(|e| panic!("assets/rules/{kind}/{date}.json is in the repository: {e}"));
        assert_eq!(&on_disk, text, "{kind}: the built-in copy is the file under assets/rules/");
        // dated, sourced and (for the circular's numbers) not a sample
        let v: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
        assert_eq!(v["as_of"].as_str(), Some(*date), "{kind}: as_of is the file's date");
        assert!(v["source"].as_str().is_some_and(|s| s.len() > 20), "{kind}: it names its source");
    }
    // the two IRRBB files carry the circular's numbers: not samples
    set_data_dir_override(Some(dir("a4_src")));
    let shocks = load_shocks(d("2026-10-31")).expect("loads");
    let caps = load_nmd_caps(d("2026-10-31")).expect("loads");
    set_data_dir_override(None);
    assert!(!shocks.meta.sample && !caps.meta.sample);
}

#[test]
fn a4_a_missing_file_is_written_from_the_built_in_copy_and_only_then() {
    for (kind, (date, text)) in SHIPPED_RULES {
        let tmp = dir(&format!("a4_missing_{kind}"));
        let folder = tmp.join("rules").join(kind);
        let file = folder.join(format!("{date}.json"));
        assert!(!file.exists());
        write_builtin_if_missing(&folder, (date, text));
        assert_eq!(std::fs::read_to_string(&file).expect("written"), *text, "{kind}: the file is a copy of the shipped one");
        // a second call changes nothing (not even the modified time of the content)
        write_builtin_if_missing(&folder, (date, text));
        assert_eq!(std::fs::read_to_string(&file).expect("still there"), *text);
        // the loaders write it too, on first use
        let tmp = dir(&format!("a4_loader_{kind}"));
        set_data_dir_override(Some(tmp.clone()));
        match *kind {
            "irrbb_shocks" => drop(load_shocks(d("2026-10-31")).expect("loads")),
            "irrbb_nmd_caps" => drop(load_nmd_caps(d("2026-10-31")).expect("loads")),
            "cl_overdue" => drop(micro_parquet::overdue::load_overdue_rules(d("2026-10-31")).expect("loads")),
            other => panic!("no loader is known for the shipped kind {other}"),
        }
        set_data_dir_override(None);
        assert!(tmp.join("rules").join(kind).join(format!("{date}.json")).exists(), "{kind}: written on first use");
    }
}

#[test]
fn a4_an_existing_file_is_never_overwritten_not_even_an_edited_or_a_broken_one() {
    for (kind, (date, text)) in SHIPPED_RULES {
        let tmp = dir(&format!("a4_keep_{kind}"));
        let folder = tmp.join("rules").join(kind);
        std::fs::create_dir_all(&folder).expect("folder");
        let file = folder.join(format!("{date}.json"));
        // an edited copy: other numbers
        let edited = text.replacen("\"as_of\"", "\"note\": \"edited by the bank\",\n  \"as_of\"", 1);
        assert_ne!(&edited, text, "{kind}: the test really edits the file");
        std::fs::write(&file, &edited).expect("write the edited copy");
        write_builtin_if_missing(&folder, (date, text));
        assert_eq!(std::fs::read_to_string(&file).expect("read"), edited, "{kind}: the edited file is left as it is");
        // a broken file is not repaired behind the owner's back either: the run says so
        std::fs::write(&file, "{ not json").expect("write a broken file");
        set_data_dir_override(Some(tmp.clone()));
        let err = match *kind {
            "irrbb_shocks" => load_shocks(d("2026-10-31")).err(),
            "irrbb_nmd_caps" => load_nmd_caps(d("2026-10-31")).err(),
            "cl_overdue" => micro_parquet::overdue::load_overdue_rules(d("2026-10-31")).err(),
            other => panic!("no loader is known for the shipped kind {other}"),
        };
        set_data_dir_override(None);
        let e = format!("{:#}", err.expect("a broken file is an error"));
        assert!(e.contains("is not valid JSON"), "{kind}: {e}");
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "{ not json", "{kind}: the broken file was not replaced");
    }
}

#[test]
fn a4_an_edited_shock_file_is_used_and_stays_edited_after_a_run() {
    let tmp = dir("a4_edited_run");
    let folder = tmp.join("rules").join("irrbb_shocks");
    std::fs::create_dir_all(&folder).expect("folder");
    let text = IRRBB_SHOCKS.1.replace("\"parallel_bp\": 400", "\"parallel_bp\": 350");
    assert_ne!(text, IRRBB_SHOCKS.1);
    std::fs::write(folder.join("2021-04-13.json"), &text).expect("write");
    set_data_dir_override(Some(tmp));
    let f = load_shocks(d("2026-10-31")).expect("loads");
    set_data_dir_override(None);
    assert_eq!(f.data.for_currency("BDT", &f.meta.file).expect("BDT").parallel_bp, 350.0, "the owner's number is used");
    assert_eq!(std::fs::read_to_string(folder.join("2021-04-13.json")).expect("read"), text, "and the file is untouched");
}
