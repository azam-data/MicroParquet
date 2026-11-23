# MicroParquet

Desktop Parquet/CSV viewer, converter and editor (Rust, eframe/egui 0.29, Polars 0.43) for bank treasury, ALM and finance work in Bangladesh. Outputs feed ALCO packs and Bangladesh Bank returns: a silently wrong number is worse than an error.

## Layout
- `src/models.rs`: types, `TransformStep`, number formats
- `src/engine.rs`: CSV ingestion, schema inference, step compiler, group-by, filters
- `src/banking.rs`: maturity buckets, ΔNII shock, FTP, hygiene audit, reconcile, duration/ΔEVE, provisioning, limit utilisation
- `src/macros.rs`: saved recipes and Python code generation
- `src/main.rs`: egui UI (large, so grep it; don't read it whole)
- `src/exporters.rs`, `src/batch.rs`: exports and batch conversion
- `portable_data/`: user presets and rule files, next to the executable
- `docs/`: specs for each phase (for example `docs/phase1-spec.md`)

## Commands
- Build: `cargo build --release`. The first build compiles Polars (10–20 min), so never run `cargo clean`.
- While editing: `cargo check`
- Tests: `cargo test` (the calculation tests are in `tests/review.rs`)

## Rules
- Don't upgrade or add dependencies unless the task says so.
- New fields on serialised types get `#[serde(default)]`, so old macro JSON still loads.
- No `unwrap()`/`expect()`/index-panics on user-data paths. Return `anyhow::Result` with a plain message a banker can act on.
- Fail loudly on bad input (wrong type, unmapped code) instead of defaulting a value.
- Change only what the task needs. Don't reformat untouched files.
- One commit per task, then push.

## Domain conventions
- CBS dates are usually D/M/Y. Money is BDT, displayed in Crore (1 Cr = 10,000,000).
- Rates are stored in percent (9.5 = 9.5%). Shocks are in basis points.
- Regulatory rates live in data files with effective dates, never hard-coded.

## Compact instructions
When compacting, keep: the task IDs in progress, which tests pass or fail, deviations from the spec, and open questions for the owner. Drop file contents and build logs.

## Working efficiently
- One or two task IDs per session; resume from the progress file.
- While editing: cargo check, then only the task's tests, e.g.
  cargo test -q --test phase3 w4 2>&1 | tail -40
- Full cargo test once before each commit, piped through tail -40.
- Don't read src/main.rs or other large files whole; grep and read ranges.
- Never paste build logs or test lists into the reply; summarise.
