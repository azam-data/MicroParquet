//! B0: the Phase 8 baseline (goal 3). The samples saved by the Phase 8 code (`tests/fixtures/phase8_replay/`, made at the tag
//! v1.8.0 by `tests/phase9/p8_baseline.rs`) load with today's code and give the hashes that code gave: the rows of three macros,
//! of the pack's sheets and of the project's 7 tabs, the numbers and the layout of the charts, and the project's bytes after
//! save → open → save (without `saved_at`).

use crate::common::fixtures_dir;
use crate::p9_p8_baseline as base;
use std::collections::BTreeMap;

#[test]
fn b0_the_phase8_samples_replay_to_the_hashes_the_phase8_code_gave() {
    let fx = fixtures_dir().join("phase8_replay");
    let want: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(fx.join("phase8_replay.hashes.json")).expect("hashes")).expect("json");
    // 3 macros, 2 pack sheets and their 2 charts, the saved chart, 7 project tabs and the project's bytes
    assert_eq!(want.len(), 3 + 2 + 2 + 1 + 7 + 1, "{:?}", want.keys().collect::<Vec<_>>());
    for k in ["macro:phase8_split_date", "macro:phase8_filter_part_gte", "macro:phase8_rate_band_unnumbered", "chart:Deposits drill", "project:bytes after save-open-save"] {
        assert!(want.contains_key(k), "{k} is in the hashes");
    }
    // no .mpproj of the fixtures points outside them
    let project = std::fs::read_to_string(fx.join("proj/phase8_replay.mpproj")).expect("the project");
    for line in project.lines().filter(|l| l.contains("\"path\":")) {
        let v = line.split('"').nth(3).unwrap_or("");
        assert!(!v.starts_with('/') && !v.contains(":\\") || v.contains("tests/fixtures/phase8_replay"), "a path outside the fixtures: {v}");
    }
    let got = base::replay(&fx, &std::env::temp_dir().join(format!("mp_b0_replay_{}", std::process::id())));
    let differ: Vec<&String> = want.keys().chain(got.keys()).filter(|k| want.get(*k) != got.get(*k)).collect();
    assert!(differ.is_empty(), "these differ from what the Phase 8 code gave: {differ:?}");
    assert_eq!(got, want);
}
