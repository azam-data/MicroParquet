//! Phase 3 tests (one test binary: each integration test file links all of Polars, about 1.5 GB in a debug build).

mod jobs;
mod l2;
mod l3;
mod l4;
mod l5;
mod r3;
mod t1;
mod t2;
mod t3;
mod t4;
mod t5;
mod t6;
mod fixes;
mod fixes2;
mod p4;

// Phase 4 tests live in tests/phase4/ but are compiled into this binary: every integration test
// binary links all of Polars, and the CI runner's disk is small.
#[path = "../phase4/common.rs"]
mod common;
#[path = "../phase4/guide.rs"]
mod guide;
#[path = "../phase4/v1.rs"]
mod v1;
#[path = "../phase4/v2.rs"]
mod v2;
#[path = "../phase4/v3.rs"]
mod v3;
mod v3_support {
    pub use crate::v3::two_bucket_preset;
    pub use micro_parquet::banking::*;
}
#[path = "../phase4/v4.rs"]
mod v4;
#[path = "../phase4/v5.rs"]
mod v5;
#[path = "../phase4/r.rs"]
mod r;

// Phase 5 tests (tests/phase5/) are compiled into this binary too, for the same reason.
#[path = "../phase5/f0.rs"]
mod f0;
#[path = "../phase5/a4.rs"]
mod a4;
#[path = "../phase5/r1.rs"]
mod r1;
#[path = "../phase5/r2.rs"]
mod r2;
#[path = "../phase5/r3.rs"]
mod r3_p5;
#[path = "../phase5/r4.rs"]
mod r4;
#[path = "../phase5/r5.rs"]
mod r5;
#[path = "../phase5/r6.rs"]
mod r6;
#[path = "../phase5/r7.rs"]
mod r7;
#[path = "../phase5/d9.rs"]
mod d9;
#[path = "../phase5/c2.rs"]
mod c2;
#[path = "../phase5/guide.rs"]
mod guide5;

// Phase 6 tests (tests/phase6/) are compiled into this binary too.
#[path = "../phase6/u3.rs"]
mod u3;
#[path = "../phase6/guide.rs"]
mod guide6;
