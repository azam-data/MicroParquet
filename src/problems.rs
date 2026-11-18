//! "N row(s) …; the first 5 values of Account_No: …" errors, the Phase 4 rule for bad values in
//! the Phase 5 tools (cash flows have their own copy of this in `cashflow.rs`).

use anyhow::{bail, Result};
use polars::prelude::*;

/// What went wrong on some rows: the message, how many rows and the first 5 keys.
struct Problem {
    what: String,
    count: usize,
    first: Vec<String>,
}

/// The problems found while reading a table; `into_result` stops with all of them.
#[derive(Default)]
pub struct Problems(Vec<Problem>);

impl Problems {
    pub fn add(&mut self, what: &str, key: impl FnOnce() -> String) {
        let slot = match self.0.iter().position(|i| i.what == what) {
            Some(p) => p,
            None => {
                self.0.push(Problem { what: what.to_string(), count: 0, first: Vec::new() });
                self.0.len() - 1
            }
        };
        let i = &mut self.0[slot];
        i.count += 1;
        if i.first.len() < 5 {
            i.first.push(key());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Ok when nothing was added; else an error "`prefix`: what on N row(s); the first 5 `key_label`: a, b."
    pub fn into_result(self, prefix: &str, key_label: &str) -> Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        let parts: Vec<String> = self.0.iter().map(|i| format!("{} on {} row(s); the first {} {}: {}.", i.what, i.count, i.first.len(), key_label, i.first.join(", "))).collect();
        bail!("{}: {}", prefix, parts.join(" "));
    }
}

/// The key of each row for messages: the value of the account-number column (the first column
/// whose name looks like one), or "row N". The second part is the label for the message
/// ("values of Account_No" or "rows").
pub fn row_keys(df: &DataFrame) -> (Vec<String>, String) {
    let names: Vec<String> = df.get_column_names().iter().map(|n| n.to_string()).collect();
    let acct = crate::banking::guess_account_column(&names);
    let vals: Option<Vec<Option<String>>> = acct.as_ref().and_then(|a| df.column(a).ok()).and_then(|c| c.cast(&DataType::String).ok()).and_then(|c| c.str().ok().map(|s| s.into_iter().map(|v| v.map(str::to_string)).collect()));
    let keys: Vec<String> = (0..df.height())
        .map(|i| match vals.as_ref().and_then(|v| v.get(i)).and_then(|v| v.as_deref()).map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => v.to_string(),
            None => format!("row {}", i + 1),
        })
        .collect();
    let label = match (&acct, &vals) {
        (Some(a), Some(_)) => format!("values of {}", a),
        _ => "rows".to_string(),
    };
    (keys, label)
}
