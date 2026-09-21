//! Yang §4.3.3 / §4.4.1 — two PARALLEL-axis cylinder laterals that CROSS along
//! a ruling at a GRAZING angle (the R0038 configuration on the real kernel,
//! reduced to a coordinate axis, full tubes and non-coplanar caps).
//!
//! A = cylinder r 1 on the z-axis, z ∈ [0, 1]; B = cylinder r 1.15 on the
//! axis through (0.16, 0), z ∈ [−0.2, 1.2]. The two cross-section circles
//! meet at x = (1 − 1.15² + 0.16²)/(2·0.16), y = ±√(1 − x²) — two rulings
//! where the surface normals differ by ≈ 3°. With parallel axes every
//! facet-pair intersection of the two prisms is an axis-parallel LINE, and
//! the two cross-section polygons zigzag across each other near the grazing
//! point (Stage-1 chord sagittas ≈ 3e-2 at r 1 against a surface separation
//! that grows only as 0.05·s), so the arrangement hands Stage 3 several
//! parallel chords for ONE exact ruling; relocating them all onto it
//! collapses the strips between them into zero-area collinear triangles —
//! R0038's `degenerate_no_longedge` `LocalRefinementRequired` STOP — or, as
//! measured here before the fix, an A-cap boundary the Stage-6 walk cannot
//! close (`s6-boundary-walk-deadend`, both variants).
//!
//! The paper's rule (§4.4.1: "the two polylines in the meshes coincide with
//! the intersection curve") is the §11 generator mint applied to a CROSSING
//! ruling: give both prisms a ruling on the exact line before the
//! arrangement (spec `yang_433_tangent_point_mesh_update.md` §13). The
//! second wall it exposed — kernel-v2's midpoint-sampled orientation oracle
//! refusing the correct 0.10-thick crescent cap — is `geom::planar_loop_
//! signed_area`. Measured sweep (`cyl_cyl_grazing_ruling_sweep`): axis
//! offsets 0.16 … 0.25 (3° … 10.7°) all failed before; all complete now with
//! exact volumes.

use std::f64::consts::PI;

use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;

const R_A: f64 = 1.0;
const R_B: f64 = 1.15;
const DELTA: f64 = 0.16;

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
    // A failed auto-union is a WARNING that leaves the tool body standalone
    // (the assay grades it ERROR); it must not pass as a completed union.
    let warnings = b.engine_warnings().to_vec();
    assert!(
        warnings.is_empty(),
        "{label}: engine warnings: {warnings:?}"
    );
}

/// Lens area of two circles of radii `r1`, `r2` whose centres are `d` apart.
fn lens_area(r1: f64, r2: f64, d: f64) -> f64 {
    let a1 = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).acos();
    let a2 = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).acos();
    let k = 0.5 * ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2)).sqrt();
    r1 * r1 * a1 + r2 * r2 * a2 - k
}

fn grazing_pair(op_cut: bool) -> ModelBuilder {
    pair_at(DELTA, op_cut)
}

fn pair_at(delta: f64, op_cut: bool) -> ModelBuilder {
    let mut b = ModelBuilder::kernel_v2();
    b.true_circle_sketch("a_sk", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, R_A)
        .unwrap();
    b.extrude("a", "a_sk", 1.0).unwrap();
    // Sketch (u, v) is not world (x, y): move the sketch plane's ORIGIN to
    // place B's axis at world (delta, 0) (the `cyl_cyl_tangent_union_kv2`
    // lesson).
    b.true_circle_sketch("b_sk", [delta, 0.0, -0.2], [0.0, 0.0, 1.0], 0.0, 0.0, R_B)
        .unwrap();
    if op_cut {
        b.extrude_cut("b", "b_sk", 1.4).unwrap();
    } else {
        b.extrude("b", "b_sk", 1.4).unwrap();
    }
    b
}

fn check(mut b: ModelBuilder, label: &str, expect_volume: f64) {
    assert_clean(&b, label);
    let handle = b.solid_handle("b").expect("merged body");
    let exact = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&handle)
        .expect("exact volume");
    assert!(
        ((exact - expect_volume) / expect_volume).abs() < 1e-9,
        "{label}: exact volume {exact} vs {expect_volume}"
    );
    let mesh = b.tessellate("b").unwrap();
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{label}: {}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{label}: {}", chi.detail);
    let v = mesh_signed_volume(&mesh).abs();
    assert!(
        ((v - expect_volume) / expect_volume).abs() < 2e-2,
        "{label}: mesh volume {v} vs {expect_volume}"
    );
}

/// R0038's operation: the taller grazing cylinder CUT from the boss leaves a
/// thin crescent prism (max thickness 0.01) bounded by the two rulings.
#[test]
fn grazing_parallel_cylinder_cut_leaves_a_crescent_prism() {
    let expect = (PI * R_A * R_A - lens_area(R_A, R_B, DELTA)) * 1.0;
    check(grazing_pair(true), "cut", expect);
}

/// The same pair as a UNION: the boss plus the taller tube, the crescent
/// surviving on A's far side.
#[test]
fn grazing_parallel_cylinder_union() {
    let expect = PI * R_B * R_B * 1.4 + (PI * R_A * R_A - lens_area(R_A, R_B, DELTA)) * 1.0;
    check(grazing_pair(false), "union", expect);
}

/// CONTROL: the same two tubes with their axes 0.5 apart cross at 25.7°, a
/// crossing the chords resolve on their own (one polygon crossing per
/// ruling) — the class the corpus already passes.
#[test]
fn well_resolved_parallel_cylinder_cut_control() {
    let expect = (PI * R_A * R_A - lens_area(R_A, R_B, 0.5)) * 1.0;
    check(pair_at(0.5, true), "control cut", expect);
}
