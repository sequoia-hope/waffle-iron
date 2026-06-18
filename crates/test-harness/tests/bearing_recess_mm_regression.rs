//! Regression: the user's real bearing-recess at mm scale (a dia-~31mm cylinder,
//! 10mm tall, with a concentric dia-~16mm circle cut 2mm into the top face).
//!
//! Before the PR-YR27-Finding-2 completion (tol_for using a Stage-0 pair-plane's
//! weld `band` instead of absolute TAU_WORK), this failed with
//!   BooleanFailed("yang-rs: geometric face resolution failed for kept triangle
//!   14 (centroid off all face surfaces …)")
//! because at ~1e-2 model scale the coplanar weld leaves a ~1.5e-10 residual on
//! the annulus-cap triangles — far inside the 1e-7 detection band Stage-0 welds
//! to, but ~100x the absolute 1e-12 TAU_WORK the membership test used.
use test_harness::ModelBuilder;

#[test]
fn bearing_recess_mm_subtract_succeeds() {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bearing_recess_mm.waffle"
    ))
    .expect("read fixture");
    let mut b = ModelBuilder::kernel_v2();
    b.load(&json).expect("load bearing_recess_mm.waffle");
    let errs: Vec<String> = b
        .engine_errors()
        .iter()
        .map(|(i, m)| format!("{i}: {m}"))
        .collect();
    assert!(errs.is_empty(), "bearing recess produced errors: {errs:?}");
    // The recessed solid: bottom cap + lateral + top annulus + recess wall +
    // recess floor = 5 faces (the body cylinder had 3).
    let (_v, _e, f) = b
        .topology_counts("Extrude")
        .or_else(|_| b.topology_counts("fcaaa4da"))
        .expect("recess topology");
    assert!(f >= 5, "recess should have >=5 faces, got {f}");
}
