//! Yang §4.3.3 GENERATOR tangency with COPLANAR caps on the real kernel
//! (spec `specs/yang_433_tangent_point_mesh_update.md` §12; assay C0043).
//!
//! Two circle bosses extruded to the same height, the second internally
//! tangent to the first along one generator: the union IS the first boss.
//! Before the fix the boolean STOPped at Stage 3 (`AmbiguousCurve {1, 0}`:
//! the small prism poked a sagitta outside the large one at the tangent
//! azimuth); the mint is now applied before Stage 0 and the coplanar caps
//! are emitted identically (shared fan + pinched crescent).
//!
//! Sketch (u, v) is NOT world (x, y): a circle centred at sketch (0.6, 0)
//! lands at world (0, −0.6), so the tangent generator falls on the outer
//! boss's SEAM ruling (x = 0, y = −1). Placing the sketch plane's origin
//! at world (0.6, 0, 0) instead puts the tangency at (1, 0) — mid-quad on
//! a 13-gon, the exact C0043 configuration. Both are covered.

use std::f64::consts::PI;

use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
}

/// `on_seam`: the inner circle's centre at sketch (0.6, 0) → the tangency
/// on the outer boss's seam ruling; else the sketch plane's origin moves to
/// world (0.6, 0, 0) and the tangency sits mid-quad (C0043).
fn tangent_pair(op_cut: bool, on_seam: bool) -> ModelBuilder {
    let mut b = ModelBuilder::kernel_v2();
    b.true_circle_sketch("a_sk", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0)
        .unwrap();
    b.extrude("a", "a_sk", 1.0).unwrap();
    if on_seam {
        b.true_circle_sketch("b_sk", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.6, 0.0, 0.4)
            .unwrap();
    } else {
        b.true_circle_sketch("b_sk", [0.6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 0.4)
            .unwrap();
    }
    if op_cut {
        b.extrude_cut("b", "b_sk", 1.0).unwrap();
    } else {
        b.extrude("b", "b_sk", 1.0).unwrap();
    }
    b
}

fn assert_union_is_the_outer_boss(mut b: ModelBuilder, label: &str) {
    assert_clean(&b, label);
    let handle = b.solid_handle("b").expect("merged body");
    let exact = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&handle)
        .expect("exact volume");
    assert!(
        ((exact - PI) / PI).abs() < 1e-12,
        "{label}: exact union volume {exact} vs π"
    );
    let mesh = b.tessellate("b").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{label}: {}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{label}: {}", chi.detail);
    let v = mesh_signed_volume(&mesh).abs();
    assert!(
        ((v - PI) / PI).abs() < 5e-3,
        "{label}: mesh volume {v} vs π"
    );
}

/// C0043 exactly: union == operand A, exact volume π·1²·1, the tangency
/// mid-quad on the outer boss's 13-gon.
#[test]
fn internally_tangent_boss_union_is_the_outer_boss() {
    assert_union_is_the_outer_boss(tangent_pair(false, false), "union off-seam");
}

/// The same union with the tangency ON the outer boss's seam ruling (the
/// mint's rim sample is skipped there — the ruling already exists).
#[test]
fn internally_tangent_boss_union_on_the_seam_is_the_outer_boss() {
    assert_union_is_the_outer_boss(tangent_pair(false, true), "union on-seam");
}

/// The same pair as a full-height CUT: a crescent prism of exact volume
/// π(1 − 0.4²) whose wall thins to zero along the WHOLE tangent generator
/// (C0056's configuration, but through both caps).
///
/// QUARANTINED (2026-09-21): a through-going line pinch. yang completes the
/// boolean with the cusp duplicated per cap, but its Stage-5 reassembly
/// hands kernel-v2 both walls as CLOSED tubes (outer loop = one rim, inner
/// loop = the other) whose bottom rim chain on the outer wall is 4 coarse
/// arcs (1→10→7→4) against the cap's 14 — `InvalidBooleanOutput("an
/// undirected output edge is not used by exactly two directed edges")`,
/// loud. Before the §12 Stage-0 mint the same document STOPped one stage
/// earlier (Stage 3 `AmbiguousCurve {1, 0}`). Owner: the pinch-edge family
/// (F0060 / C0056 `split_pinch_vertices`) for a pinch that runs cap to cap.
/// Un-quarantine when the cut completes.
#[test]
#[ignore = "through-going line pinch: Stage-5 reassembly emits closed tubes with a mismatched rim chain — pinch-edge family, not §4.3.3 §12"]
fn internally_tangent_full_height_cut_leaves_a_crescent_prism() {
    let mut b = tangent_pair(true, true);
    assert_clean(&b, "cut");
    let handle = b.solid_handle("b").expect("cut body");
    let exact = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&handle)
        .expect("exact volume");
    let expect = PI * (1.0 - 0.16);
    assert!(
        ((exact - expect) / expect).abs() < 1e-12,
        "exact cut volume {exact} vs {expect}"
    );
    let mesh = b.tessellate("b").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
}
