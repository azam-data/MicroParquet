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
#[path = "../phase6/u1.rs"]
mod u1;
#[path = "../phase6/u5.rs"]
mod u5;
#[path = "../phase6/u7.rs"]
mod u7;
#[path = "../phase6/p1.rs"]
mod p1;
#[path = "../phase6/p2.rs"]
mod p2;
#[path = "../phase6/p3.rs"]
mod p3;
#[path = "../phase6/p4.rs"]
mod p4_dq;
#[path = "../phase6/p5.rs"]
mod p5_islamic;
#[path = "../phase6/p6.rs"]
mod p6_stress;
#[path = "../phase6/k2.rs"]
mod k2;
#[path = "../phase6/k3.rs"]
mod k3;
#[path = "../phase6/k4.rs"]
mod k4;
#[path = "../phase6/l2.rs"]
mod l2_pdf;
#[path = "../phase6/guide.rs"]
mod guide6;

// Phase 7 tests (tests/phase7/) are compiled into this binary too.
#[path = "../phase7/c0.rs"]
mod p7_c0;
#[path = "../phase7/c1.rs"]
mod p7_c1;
#[path = "../phase7/c2.rs"]
mod p7_c2;
#[path = "../phase7/d.rs"]
mod p7_d;
#[path = "../phase7/c3.rs"]
mod p7_c3;
#[path = "../phase7/c4.rs"]
mod p7_c4;
#[path = "../phase7/c5.rs"]
mod p7_c5;
#[path = "../phase7/f.rs"]
mod p7_f;
#[path = "../phase7/c6.rs"]
mod p7_c6;
#[path = "../phase7/c7.rs"]
mod p7_c7;
#[path = "../phase7/c8.rs"]
mod p7_c8;
#[path = "../phase7/g.rs"]
mod p7_g;
#[path = "../phase7/r.rs"]
mod p7_r;
#[path = "../phase7/s.rs"]
mod p7_s;

// Phase 8 tests (tests/phase8/) are compiled into this binary too.
#[path = "../phase8/common8.rs"]
mod common8;
#[path = "../phase8/p7_baseline.rs"]
mod p7_baseline;
#[path = "../phase8/w0.rs"]
mod p8_w0;
#[path = "../phase8/w1.rs"]
mod p8_w1;
#[path = "../phase8/w2.rs"]
mod p8_w2;
#[path = "../phase8/q.rs"]
mod p8_q;
#[path = "../phase8/w3.rs"]
mod p8_w3;
#[path = "../phase8/w5.rs"]
mod p8_w5;
#[path = "../phase8/w6.rs"]
mod p8_w6;
#[path = "../phase8/w4.rs"]
mod p8_w4;
#[path = "../phase8/w7.rs"]
mod p8_w7;
