//! Boolean determinism tests.
//!
//! Verifies that boolean operations produce identical results across
//! multiple runs, regardless of memory allocation patterns or hash map
//! iteration order.
//!
//! Remediation plan O7 (2026-10-29): "identical" now means BIT-identical.
//! Each run is a fresh adapter; every run's result must have the same
//! canonical B-Rep hash (`test_harness::assay::geometry_hash` — vertex
//! positions, edge polylines, face signatures and exact volume as `f64`
//! bits, id- and order-independent) and the same render mesh (triangle
//! count and mesh volume, both bit-exact). The old check — triangle count
//! plus mesh volume within 1.0 absolute on a ~1000-unit solid — could not
//! see a different solid with the same triangle count.
//!
//! With `WAFFLE_GEOMETRY_HASH_OUT` set, each scenario's hash is appended to
//! that file so `scripts/determinism_parallel_parity.sh` can diff a serial
//! and a `cherchi-rs/parallel` build.

use test_harness::assay::geometry_hash::{feature_geometry_hash, record_hash_for_parity};
use test_harness::helpers::mesh_volume;
use test_harness::ModelBuilder;

/// One run's fingerprint: B-Rep hash, triangle count, mesh volume bits.
type RunPrint = (u64, usize, u64);

fn fingerprint(m: &mut ModelBuilder, feature: &str) -> RunPrint {
    let hash = feature_geometry_hash(m, feature).unwrap();
    let mesh = m.tessellate(feature).unwrap();
    (hash, mesh.indices.len() / 3, mesh_volume(&mesh).to_bits())
}

/// Assert all runs are bit-identical, then record the hash for the
/// cross-configuration parity diff.
fn assert_identical(label: &str, runs: &[RunPrint]) {
    let first = runs[0];
    for (i, r) in runs.iter().enumerate() {
        assert_eq!(
            *r, first,
            "{label} run {i}: (hash, triangles, volume bits) {:016x?} != run 0 {:016x?}",
            r, first
        );
    }
    record_hash_for_parity(
        &format!("boolean_determinism/{label}"),
        &format!("{:016x}", first.0),
    );
}

/// Run a boss union boolean N times and verify every run is bit-identical.
#[test]
fn boolean_union_deterministic_10x() {
    let mut runs = Vec::new();

    for _ in 0..10 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Boss on top face — triggers coplanar boolean
        m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
            .unwrap();
        m.extrude("boss", "boss_sk", 5.0).unwrap();

        runs.push(fingerprint(&mut m, "boss"));
    }
    assert_identical("union", &runs);
}

/// Run a subtract boolean N times and verify deterministic results.
#[test]
fn boolean_subtract_deterministic_10x() {
    let mut runs = Vec::new();

    for _ in 0..10 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Cut pocket in center of top face
        m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
            .unwrap();
        m.extrude_cut("pocket", "cut_sk", 5.0).unwrap();

        runs.push(fingerprint(&mut m, "pocket"));
    }
    assert_identical("subtract", &runs);
}

/// Chained boolean determinism: boss then cut, repeated.
#[test]
fn chained_boolean_deterministic_10x() {
    let mut runs = Vec::new();

    for _ in 0..10 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Two cuts on top face
        m.rect_sketch("cut1_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
            .unwrap();
        m.extrude_cut("cut1", "cut1_sk", 5.0).unwrap();

        m.rect_sketch("cut2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
            .unwrap();
        m.extrude_cut("cut2", "cut2_sk", 5.0).unwrap();

        runs.push(fingerprint(&mut m, "cut2"));
    }
    assert_identical("chained", &runs);
}
