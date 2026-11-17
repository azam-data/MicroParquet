//! Phase 4 tests (one test binary: each integration test file links all of Polars, about 1.5 GB in a
//! debug build). Modules: `guide` (G0 guide notes), then one per task.

mod common;
mod guide;
mod v1;
mod v2;
mod v3;
mod v3_support {
    pub use crate::v3::two_bucket_preset;
    pub use micro_parquet::banking::*;
}
