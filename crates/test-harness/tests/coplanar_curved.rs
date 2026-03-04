//! Coplanar curved-face boolean tests for TruckKernel.
//!
//! These tests document a bug where `face_boundary_vertices()` in
//! `coplanar_splitting.rs` returns only topological vertices (1-2 for circular
//! boundaries), causing `face_normal_from_vertices()` to return `None` when
//! < 3 points are available. This makes coplanar detection silently fail for
//! ANY face pair with circular boundaries.
//!
//! The fix pattern exists: `dense_sample_wire_points()` in `coplanar.rs`
//! samples 32 points per edge when vertex count < 3. It's used in
//! `classify_coplanar_fragment` and `face_interior_point` but NOT in
//! `face_boundary_vertices`.
//!
//! All tests are `#[ignore]` per FIP Phase 2 (test-first).
//!
//! Categories:
//!   CPC — Cylinder-on-Cylinder Cuts (4 tests)
//!   CPB — Boss-then-Cut chains (2 tests)
//!   CPE — Explicit Boolean between Cylinders (2 tests)
//!   CPU — Coplanar Curved-Face Unions (2 tests)

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Approximate volume of a 16-segment polygon cylinder.
/// area = r^2 * 16 * sin(2pi/16) / 2, then * h.
fn approx_cylinder_volume(r: f64, h: f64) -> f64 {
    let n = 16.0_f64;
    let area = r * r * n * (2.0 * std::f64::consts::PI / n).sin() / 2.0;
    area * h
}

/// Assert all mesh vertices and normals are finite (no NaN/Inf).
fn assert_mesh_finite(mesh: &kernel_fork::types::RenderMesh, label: &str) {
    for (i, v) in mesh.vertices.iter().enumerate() {
        assert!(
            v.is_finite(),
            "{}: vertex[{}] is not finite: {}",
            label,
            i,
            v
        );
    }
    for (i, n) in mesh.normals.iter().enumerate() {
        assert!(
            n.is_finite(),
            "{}: normal[{}] is not finite: {}",
            label,
            i,
            n
        );
    }
}

/// Create a 10x10x10 base cube at origin on XY plane.
fn base_cube() -> ModelBuilder {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

// ══════════════════════════════════════════════════════════════════════════════
// CPE — Explicit Boolean between Standalone Cylinders
// ══════════════════════════════════════════════════════════════════════════════

/// CPE1: Two concentric cylinders (r=5, r=2), same height, explicit subtract.
///
/// Simplest coplanar curved case: both caps are circles sharing the same plane.
/// Top faces: circle r=5 vs circle r=2, both at z=20. Both have ~1 topological
/// vertex, so `face_boundary_vertices` returns too few points for normal computation.
///
/// Expected: tube with vol ≈ cyl(5,20) - cyl(2,20).
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpe1_concentric_cylinders_subtract() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2, h=20 (concentric)
    m.circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 20.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Subtract inner from outer
    m.boolean_subtract("tube", "outer", "inner").unwrap();
    m.assert_has_solid("tube").unwrap();

    // Volume oracle
    let mesh = m.tessellate("tube").unwrap();
    assert_mesh_finite(&mesh, "cpe1");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(2., 20.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPE1: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Topology: tube = genus-1 torus, V-E+F = 0
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 0,
        "CPE1: Tube Euler characteristic should be 0, got {}",
        chi
    );
}

/// CPE2: Two offset cylinders (r=5 at origin, r=1.5 at (3,0)), explicit subtract.
///
/// Off-center cut — the inner circle boundary partially overlaps the outer cap
/// but is fully contained. Both caps share the z=0 and z=20 planes.
///
/// Expected: tube with vol ≈ cyl(5,20) - cyl(1.5,20).
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpe2_offset_cylinders_subtract() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=1.5, offset at (3, 0), h=20
    m.circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 3., 0., 1.5)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 20.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Subtract
    m.boolean_subtract("tube", "outer", "inner").unwrap();
    m.assert_has_solid("tube").unwrap();

    // Volume oracle
    let mesh = m.tessellate("tube").unwrap();
    assert_mesh_finite(&mesh, "cpe2");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(1.5, 20.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPE2: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Topology: tube = genus-1, V-E+F = 0
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 0,
        "CPE2: Tube Euler characteristic should be 0, got {}",
        chi
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// CPC — Cylinder-on-Cylinder Cuts (user workflow scenarios)
// ══════════════════════════════════════════════════════════════════════════════

/// CPC1: Concentric circle cut from cylinder top, full depth (tube).
///
/// This is the canonical user-reported failure: extrude a circle to make a
/// cylinder, then cut a smaller concentric circle through the full depth.
/// Coplanar pair: cylinder cap (circle, 1 vertex) vs cut cap (circle, 1 vertex).
///
/// Expected: tube with vol ≈ cyl(5,20) - cyl(2,20).
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpc1_concentric_cylinder_cut_full_depth() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2, same origin, h=20
    m.circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 20.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Subtract
    m.boolean_subtract("tube", "outer", "inner").unwrap();
    m.assert_has_solid("tube").unwrap();

    // Volume oracle
    let mesh = m.tessellate("tube").unwrap();
    assert_mesh_finite(&mesh, "cpc1");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(2., 20.);
    assert!(
        (vol - expected).abs() < expected * 0.05,
        "CPC1: volume {:.1} not within 5% of expected {:.1}",
        vol,
        expected
    );

    // Topology: tube = genus-1, V-E+F = 0
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 0,
        "CPC1: Tube Euler characteristic should be 0, got {}",
        chi
    );
}

/// CPC2: Concentric cylinder cut, partial depth (blind hole).
///
/// Outer cylinder r=5 h=20, inner cut r=2 depth=10 from top (z=20 down to z=10).
/// Creates a pocket, not a through-hole. Top face is still coplanar circle-circle.
///
/// Expected: vol ≈ cyl(5,20) - cyl(2,10), genus-0 (no through-hole), V-E+F = 2.
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpc2_concentric_cylinder_cut_partial_depth() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2, h=10, placed at top face z=20, cuts downward
    m.circle_sketch("inner_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 10.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Subtract (pocket)
    m.boolean_subtract("pocket", "outer", "inner").unwrap();
    m.assert_has_solid("pocket").unwrap();

    // Volume oracle: full cylinder minus partial cut
    let mesh = m.tessellate("pocket").unwrap();
    assert_mesh_finite(&mesh, "cpc2");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(2., 10.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPC2: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Topology: pocket is genus-0 (no through-hole), V-E+F = 2
    let (v, e, f) = m.topology_counts("pocket").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPC2: Pocket Euler characteristic should be 2, got {}",
        chi
    );
}

/// CPC3: Off-center cylinder cut, full depth through.
///
/// Outer cylinder r=5 h=20, cut r=1.5 at center (2, 1), full depth.
/// The inner circle is fully contained in the outer cap face.
///
/// Expected: tube with vol ≈ cyl(5,20) - cyl(1.5,20).
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpc3_offset_cylinder_cut_full_depth() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=1.5, offset at (2, 1), h=20
    m.circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 2., 1., 1.5)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 20.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Subtract
    m.boolean_subtract("tube", "outer", "inner").unwrap();
    m.assert_has_solid("tube").unwrap();

    // Volume oracle
    let mesh = m.tessellate("tube").unwrap();
    assert_mesh_finite(&mesh, "cpc3");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(1.5, 20.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPC3: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Topology: tube = genus-1, V-E+F = 0
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 0,
        "CPC3: Tube Euler characteristic should be 0, got {}",
        chi
    );
}

/// CPC4: Concentric cylinder cut via feature-engine pipeline (extrude + extrude_cut).
///
/// This is the exact user workflow: sketch a circle on the XY plane, extrude it,
/// then sketch a smaller circle on the top face and extrude-cut it.
/// Uses auto-merge `extrude` + `extrude_cut` instead of explicit booleans.
///
/// Expected: vol ≈ cyl(5,20) - cyl(2,20), no engine errors.
#[test]
fn cpc4_concentric_cut_via_feature_engine() {
    let mut m = ModelBuilder::truck();

    // Step 1: Extrude outer cylinder (auto-merge, becomes base body)
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Step 2: Sketch on top face (z=20) and extrude-cut
    m.circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "cpc4");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(2., 20.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPC4: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Bounding box sanity: should still be centered at origin, height 20
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 19.0 && bb_max[2] < 21.0,
        "CPC4: top of cylinder should be near z=20, got z={}",
        bb_max[2]
    );
    assert!(
        bb_min[2] > -1.0 && bb_min[2] < 1.0,
        "CPC4: bottom of cylinder should be near z=0, got z={}",
        bb_min[2]
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// CPB — Boss-then-Cut chains (3-operation sequences)
// ══════════════════════════════════════════════════════════════════════════════

/// CPB1: Box + cylinder boss + cylinder cut from boss top.
///
/// 10x10x10 cube, then circle boss r=3 h=5 on top (z=10 to z=15),
/// then circle cut r=1.5 h=5 from boss top (z=15 down to z=10).
/// The cut's top face is coplanar with the boss's top face.
///
/// Expected: vol ≈ 10^3 + cyl(3,5) - cyl(1.5,5).
#[test]
fn cpb1_box_boss_cut_from_boss_top() {
    let mut m = base_cube();

    // Boss: circle r=3 on top of cube (z=10), extrude up 5 units
    m.circle_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // Cut: circle r=1.5, concentric with boss, on boss top (z=15), cut depth=5
    m.circle_sketch("cut_sk", [0., 0., 15.], [0., 0., 1.], 5., 5., 1.5)
        .unwrap();
    m.extrude_cut("pocket", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("pocket").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("pocket").unwrap();
    assert_mesh_finite(&mesh, "cpb1");
    let vol = mesh_volume(&mesh);
    let cube_vol = 10.0 * 10.0 * 10.0;
    let expected = cube_vol + approx_cylinder_volume(3., 5.) - approx_cylinder_volume(1.5, 5.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPB1: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Bounding box: should extend to z=15 (boss top)
    let (_, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 14.0 && bb_max[2] < 16.0,
        "CPB1: top should be near z=15, got z={}",
        bb_max[2]
    );
}

/// CPB2: Box + cylinder boss + deep cylinder cut through everything.
///
/// Same as CPB1 but the cut goes depth=15 from z=15 through the boss and cube.
/// The cut tool cylinder's bottom face (z=0) is coplanar with the cube bottom.
///
/// Expected: vol ≈ 10^3 + cyl(3,5) - cyl(1.5,15).
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpb2_box_boss_deep_cut_through() {
    let mut m = base_cube();

    // Boss: circle r=3 on top of cube (z=10), extrude up 5 units
    m.circle_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // Deep cut: circle r=1.5, from boss top z=15, depth=15 (all the way through)
    m.circle_sketch("cut_sk", [0., 0., 15.], [0., 0., 1.], 5., 5., 1.5)
        .unwrap();
    m.extrude_cut("through_hole", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("through_hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("through_hole").unwrap();
    assert_mesh_finite(&mesh, "cpb2");
    let vol = mesh_volume(&mesh);
    let cube_vol = 10.0 * 10.0 * 10.0;
    let expected = cube_vol + approx_cylinder_volume(3., 5.) - approx_cylinder_volume(1.5, 15.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPB2: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Through-hole makes genus-1 topology: V-E+F = 0
    let (v, e, f) = m.topology_counts("through_hole").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 0,
        "CPB2: Through-hole Euler characteristic should be 0, got {}",
        chi
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// CPU — Coplanar Curved-Face Unions
// ══════════════════════════════════════════════════════════════════════════════

/// CPU1: Union of two concentric cylinders (r=5, r=2), same height.
///
/// Since the inner cylinder is fully contained, the union should equal the
/// outer cylinder. This tests that coplanar detection works for union as well
/// as subtraction.
///
/// Expected: vol ≈ cyl(5,20) (outer contains inner).
#[test]
#[ignore = "Coplanar curved-face bug: face_boundary_vertices() returns 1-2 topological \
            vertices for circular boundaries, causing check_coplanar_faces() to bail \
            (normal computation needs >=3 points). Fix: use dense_sample_wire_points()."]
fn cpu1_concentric_cylinders_union() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2, h=20 (concentric, fully contained)
    m.circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 20.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Union
    m.boolean_union("merged", "outer", "inner").unwrap();
    m.assert_has_solid("merged").unwrap();

    // Volume oracle: union of contained shapes = outer volume
    let mesh = m.tessellate("merged").unwrap();
    assert_mesh_finite(&mesh, "cpu1");
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.);
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "CPU1: volume {:.1} not within 10% of expected {:.1} (should equal outer)",
        vol,
        expected
    );

    // Topology: simple solid, V-E+F = 2
    let (v, e, f) = m.topology_counts("merged").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPU1: Union Euler characteristic should be 2, got {}",
        chi
    );
}

/// CPU2: Union of two overlapping same-radius cylinders (r=3, offset by 2).
///
/// Two r=3 h=20 cylinders, one at origin, one at (2,0). They overlap
/// significantly. Union volume should be between 1x and 2x single cylinder.
///
/// Expected: cyl(3,20) < vol < 2*cyl(3,20).
#[test]
fn cpu2_overlapping_cylinders_union() {
    let mut m = ModelBuilder::truck();

    // Cylinder A: r=3, h=20, centered at origin
    m.circle_sketch("cyl_a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 3.)
        .unwrap();
    m.extrude_no_merge("cyl_a", "cyl_a_sk", 20.0).unwrap();
    m.assert_has_solid("cyl_a").unwrap();

    // Cylinder B: r=3, h=20, centered at (2, 0)
    m.circle_sketch("cyl_b_sk", [0., 0., 0.], [0., 0., 1.], 2., 0., 3.)
        .unwrap();
    m.extrude_no_merge("cyl_b", "cyl_b_sk", 20.0).unwrap();
    m.assert_has_solid("cyl_b").unwrap();

    // Union
    m.boolean_union("merged", "cyl_a", "cyl_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    // Volume bounds: strictly between 1x and 2x single cylinder
    let mesh = m.tessellate("merged").unwrap();
    assert_mesh_finite(&mesh, "cpu2");
    let vol = mesh_volume(&mesh);
    let single = approx_cylinder_volume(3., 20.);
    assert!(
        vol > single * 0.95,
        "CPU2: union volume {:.1} should be >= single cylinder {:.1}",
        vol,
        single
    );
    assert!(
        vol < single * 2.05,
        "CPU2: union volume {:.1} should be <= 2x single cylinder {:.1}",
        vol,
        single * 2.0
    );

    // Topology: simple solid, V-E+F = 2
    let (v, e, f) = m.topology_counts("merged").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPU2: Union Euler characteristic should be 2, got {}",
        chi
    );
}
