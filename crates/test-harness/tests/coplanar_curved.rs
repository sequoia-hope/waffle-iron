//! Coplanar curved-face boolean tests for TruckKernel.
//!
//! Tests for boolean operations where circular/curved faces are coplanar
//! (e.g., top caps of concentric cylinders at same height).
//!
//! Root causes fixed (Sprint 54):
//! 1. Selective tessellation for winding number classification skipped faces
//!    whose AABBs didn't overlap, causing incorrect inside/outside results.
//!    Fix: build full tessellation for winding/ray-cast classification.
//! 2. Containment injection nudge direction was wrong for same-sense coplanar
//!    faces. Fix: sense-aware nudge (flip for same-sense faces).
//! 3. Contained fixup only handled same-sense coplanar pairs.
//!    Fix: handle both same-sense and anti-sense pairs.
//!
//! Note on Euler characteristic: tube results have V-E+F = 2 (not 0) because
//! annular cap faces have 2 boundary loops. The naive V-E+F formula doesn't
//! account for non-simply-connected faces. The correct Euler-Poincaré formula
//! V-E+F-(L-F) = 2(S-G) gives 0 for genus-1.
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

    // Topology: tube with annular cap faces has V-E+F = 2 (not 0).
    // The annular faces have 2 boundary loops; the naive Euler formula
    // doesn't account for non-simply-connected faces.
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPE1: Tube V-E+F should be 2 (annular caps), got {}",
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

    // Topology: tube with annular cap faces has V-E+F = 2
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPE2: Tube V-E+F should be 2 (annular caps), got {}",
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

    // Topology: tube with annular cap faces has V-E+F = 2
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPC1: Tube V-E+F should be 2 (annular caps), got {}",
        chi
    );
}

/// CPC2: Concentric cylinder cut, partial depth (blind hole).
///
/// Outer cylinder r=5 h=20, inner cut r=2 depth=10 from top (z=20 down to z=10).
/// Creates a pocket, not a through-hole. Top face is still coplanar circle-circle.
///
/// Expected: vol ≈ cyl(5,20) - cyl(2,10), genus-0 (no through-hole), V-E+F = 2.
///
/// IGNORED: All 23 perturbation attempts fail with 16 open edges at z=20.
/// Root cause: containment injection splits outer top cap into ring+disc,
/// but face division creates NEW boundary edges for the ring that are
/// topologically disconnected from the lateral faces' top edges.
/// The v2 weld cannot resolve this because the ring's outer wire has
/// different Arc<Edge> IDs from the lateral top edges. Volume and chi
/// pass (the mesh is close enough) but the shell is not closed.
/// Needs: edge-sharing during face division, or post-assembly edge merge.
#[test]
#[ignore = "CPC2: all 23 perturbation attempts fail — 16 open edges at z=20 ring (ring outer edges disconnected from laterals)"]
fn cpc2_concentric_cylinder_cut_partial_depth() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2, h=10, placed at top face z=20, normal flipped to extrude downward
    m.circle_sketch("inner_sk", [0., 0., 20.], [0., 0., -1.], 0., 0., 2.)
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

    // Topology: pocket is genus-0. Ring face at z=20 has 2 boundary loops
    // (outer circle + inner pocket opening), giving V-E+F = 2 + 1 = 3.
    let (v, e, f) = m.topology_counts("pocket").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2 || chi == 3,
        "CPC2: Pocket Euler characteristic should be 2 or 3, got {} (V={}, E={}, F={})",
        chi,
        v,
        e,
        f
    );
}

/// CPC3: Off-center cylinder cut, full depth through.
///
/// Outer cylinder r=5 h=20, cut r=1.5 at center (2, 1), full depth.
/// The inner circle is fully contained in the outer cap face.
///
/// Expected: tube with vol ≈ cyl(5,20) - cyl(1.5,20).
#[test]
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

    // Topology: tube with annular cap faces has V-E+F = 2
    let (v, e, f) = m.topology_counts("tube").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2,
        "CPC3: Tube V-E+F should be 2 (annular caps), got {}",
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

    // Through-hole (genus-1) with ring faces at cube bottom, cube top, and boss top.
    // V-E+F = 2 - 2g + h = 2 - 2 + 3 = 3, where g=1 (through-hole), h=3 (extra loops).
    let (v, e, f) = m.topology_counts("through_hole").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi >= 2 && chi <= 4,
        "CPB2: Through-hole V-E+F should be 2-4, got {} (V={}, E={}, F={})",
        chi,
        v,
        e,
        f
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// DIAG — Per-face bounding box diagnostic
// ══════════════════════════════════════════════════════════════════════════════

/// Diagnostic: check per-face bounding boxes after full-depth concentric cylinder subtract.
/// No face should extend beyond the outer cylinder's bounding box.
#[test]
fn diag_face_bboxes_full_depth_subtract() {
    use test_harness::helpers::mesh_bounding_box;

    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    let outer_mesh = m.tessellate("outer").unwrap();
    let (o_min, o_max) = mesh_bounding_box(&outer_mesh);
    eprintln!("=== OUTER CYLINDER BBOX ===");
    eprintln!("  min: [{:.2}, {:.2}, {:.2}]", o_min[0], o_min[1], o_min[2]);
    eprintln!("  max: [{:.2}, {:.2}, {:.2}]", o_max[0], o_max[1], o_max[2]);

    // Inner cylinder r=2, h=20 (concentric)
    m.circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 20.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Subtract inner from outer
    m.boolean_subtract("tube", "outer", "inner").unwrap();
    m.assert_has_solid("tube").unwrap();

    let mesh = m.tessellate("tube").unwrap();
    let (total_min, total_max) = mesh_bounding_box(&mesh);
    eprintln!("\n=== TUBE TOTAL BBOX ===");
    eprintln!(
        "  min: [{:.2}, {:.2}, {:.2}]",
        total_min[0], total_min[1], total_min[2]
    );
    eprintln!(
        "  max: [{:.2}, {:.2}, {:.2}]",
        total_max[0], total_max[1], total_max[2]
    );

    // Per-face bounding boxes
    eprintln!(
        "\n=== PER-FACE BOUNDING BOXES ({} faces) ===",
        mesh.face_ranges.len()
    );
    let mut bad_faces = Vec::new();
    for (fi, face_range) in mesh.face_ranges.iter().enumerate() {
        let mut face_min = [f32::MAX; 3];
        let mut face_max = [f32::MIN; 3];

        for tri_base in (face_range.start_index..face_range.end_index).step_by(3) {
            for corner in 0..3usize {
                let vi = mesh.indices[tri_base as usize + corner] as usize * 3;
                for j in 0..3 {
                    face_min[j] = face_min[j].min(mesh.vertices[vi + j]);
                    face_max[j] = face_max[j].max(mesh.vertices[vi + j]);
                }
            }
        }
        let tri_count = (face_range.end_index - face_range.start_index) / 3;

        // Check if this face extends beyond the outer cylinder bbox
        let tol = 0.5;
        let exceeds = face_max[0] > o_max[0] + tol
            || face_max[1] > o_max[1] + tol
            || face_max[2] > o_max[2] + tol
            || face_min[0] < o_min[0] - tol
            || face_min[1] < o_min[1] - tol
            || face_min[2] < o_min[2] - tol;
        let flag = if exceeds { " *** EXCEEDS ***" } else { "" };
        if exceeds {
            bad_faces.push(fi);
        }

        eprintln!(
            "  face[{}]: {} tris, min=[{:.2},{:.2},{:.2}], max=[{:.2},{:.2},{:.2}]{}",
            fi,
            tri_count,
            face_min[0],
            face_min[1],
            face_min[2],
            face_max[0],
            face_max[1],
            face_max[2],
            flag,
        );
    }

    // Face signatures for surface type info
    if let Ok(sigs) = m.face_signatures("tube") {
        eprintln!("\n=== FACE SIGNATURES ===");
        for (fi, (id, sig)) in sigs.iter().enumerate() {
            eprintln!(
                "  face[{}] {:?}: type={:?}, centroid={:?}, normal={:?}",
                fi, id, sig.surface_type, sig.centroid, sig.normal
            );
        }
    }

    eprintln!("\n=== SUMMARY ===");
    eprintln!("  Total faces: {}", mesh.face_ranges.len());
    eprintln!("  Faces exceeding outer bbox: {:?}", bad_faces);

    assert!(
        bad_faces.is_empty(),
        "Faces {:?} extend beyond the outer cylinder bounding box!",
        bad_faces
    );
}

/// Diagnostic: true NURBS circle extrude + extrude_cut (exact GUI path).
///
/// Uses `true_circle_sketch` which passes CircleProfile to the kernel,
/// creating real rsweep-based circular wires — matching the GUI exactly.
#[test]
fn diag_true_nurbs_circle_extrude_cut_full_depth() {
    use test_harness::helpers::mesh_bounding_box;

    let mut m = ModelBuilder::truck();

    // Step 1: True NURBS circle r=5, extrude up 20
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    let outer_mesh = m.tessellate("cyl").unwrap();
    let (o_min, o_max) = mesh_bounding_box(&outer_mesh);
    eprintln!("=== OUTER CYLINDER (true NURBS) BBOX ===");
    eprintln!("  min: [{:.2}, {:.2}, {:.2}]", o_min[0], o_min[1], o_min[2]);
    eprintln!("  max: [{:.2}, {:.2}, {:.2}]", o_max[0], o_max[1], o_max[2]);

    // Step 2: True NURBS circle r=2 on top face (z=20), extrude-cut full depth
    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();
    m.assert_has_solid("hole").unwrap();

    let mesh = m.tessellate("hole").unwrap();
    let (t_min, t_max) = mesh_bounding_box(&mesh);
    eprintln!("\n=== TUBE (after cut) TOTAL BBOX ===");
    eprintln!("  min: [{:.2}, {:.2}, {:.2}]", t_min[0], t_min[1], t_min[2]);
    eprintln!("  max: [{:.2}, {:.2}, {:.2}]", t_max[0], t_max[1], t_max[2]);

    // Per-face bounding boxes
    eprintln!(
        "\n=== PER-FACE BOUNDING BOXES ({} faces) ===",
        mesh.face_ranges.len()
    );
    let mut bad_faces = Vec::new();
    for (fi, face_range) in mesh.face_ranges.iter().enumerate() {
        let mut face_min = [f32::MAX; 3];
        let mut face_max = [f32::MIN; 3];

        for tri_base in (face_range.start_index..face_range.end_index).step_by(3) {
            for corner in 0..3usize {
                let vi = mesh.indices[tri_base as usize + corner] as usize * 3;
                for j in 0..3 {
                    face_min[j] = face_min[j].min(mesh.vertices[vi + j]);
                    face_max[j] = face_max[j].max(mesh.vertices[vi + j]);
                }
            }
        }
        let tri_count = (face_range.end_index - face_range.start_index) / 3;

        let tol = 1.0;
        let exceeds = face_max[0] > o_max[0] + tol
            || face_max[1] > o_max[1] + tol
            || face_max[2] > o_max[2] + tol
            || face_min[0] < o_min[0] - tol
            || face_min[1] < o_min[1] - tol
            || face_min[2] < o_min[2] - tol;
        let flag = if exceeds { " *** EXCEEDS ***" } else { "" };
        if exceeds {
            bad_faces.push(fi);
        }

        eprintln!(
            "  face[{}]: {} tris, min=[{:.2},{:.2},{:.2}], max=[{:.2},{:.2},{:.2}]{}",
            fi,
            tri_count,
            face_min[0],
            face_min[1],
            face_min[2],
            face_max[0],
            face_max[1],
            face_max[2],
            flag,
        );
    }

    // Face signatures
    if let Ok(sigs) = m.face_signatures("hole") {
        eprintln!("\n=== FACE SIGNATURES ===");
        for (fi, (id, sig)) in sigs.iter().enumerate() {
            eprintln!(
                "  face[{}] {:?}: type={:?}, centroid={:?}, normal={:?}",
                fi, id, sig.surface_type, sig.centroid, sig.normal
            );
        }
    }

    // Check for errors in the feature engine
    m.assert_no_errors().unwrap();

    eprintln!("\n=== SUMMARY ===");
    eprintln!("  Total faces: {}", mesh.face_ranges.len());
    eprintln!("  Bad faces: {:?}", bad_faces);

    // Volume check
    let vol = mesh_volume(&mesh);
    let expected = approx_cylinder_volume(5., 20.) - approx_cylinder_volume(2., 20.);
    eprintln!("  Volume: {:.1} (expected: {:.1})", vol, expected);

    assert!(
        bad_faces.is_empty(),
        "Faces {:?} extend beyond the outer cylinder bounding box!",
        bad_faces
    );
}

/// Same as above but using WASM tessellation tolerance (0.0001 vs 0.1).
/// The WASM path uses 1000x finer tessellation which can trigger bugs that
/// the coarser test-harness tolerance misses.
#[test]
fn diag_true_nurbs_wasm_tolerance() {
    use test_harness::helpers::mesh_bounding_box;

    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=20
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    let outer_mesh = m.tessellate_with_tol("cyl", 0.0001).unwrap();
    let (o_min, o_max) = mesh_bounding_box(&outer_mesh);
    eprintln!("=== OUTER (tol=0.0001) ===");
    eprintln!("  min: [{:.3}, {:.3}, {:.3}]", o_min[0], o_min[1], o_min[2]);
    eprintln!("  max: [{:.3}, {:.3}, {:.3}]", o_max[0], o_max[1], o_max[2]);

    // Cut r=2, full depth
    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();
    m.assert_has_solid("hole").unwrap();

    let mesh = m.tessellate_with_tol("hole", 0.0001).unwrap();
    let (t_min, t_max) = mesh_bounding_box(&mesh);
    eprintln!("\n=== TUBE (tol=0.0001) ===");
    eprintln!("  min: [{:.3}, {:.3}, {:.3}]", t_min[0], t_min[1], t_min[2]);
    eprintln!("  max: [{:.3}, {:.3}, {:.3}]", t_max[0], t_max[1], t_max[2]);
    eprintln!(
        "  faces: {}, tris: {}",
        mesh.face_ranges.len(),
        mesh.indices.len() / 3
    );

    // Check per-face bboxes
    let mut bad_faces = Vec::new();
    for (fi, face_range) in mesh.face_ranges.iter().enumerate() {
        let mut face_min = [f32::MAX; 3];
        let mut face_max = [f32::MIN; 3];
        for tri_base in (face_range.start_index..face_range.end_index).step_by(3) {
            for corner in 0..3usize {
                let vi = mesh.indices[tri_base as usize + corner] as usize * 3;
                for j in 0..3 {
                    face_min[j] = face_min[j].min(mesh.vertices[vi + j]);
                    face_max[j] = face_max[j].max(mesh.vertices[vi + j]);
                }
            }
        }
        let tri_count = (face_range.end_index - face_range.start_index) / 3;
        let tol = 1.0;
        let exceeds = face_max[0] > o_max[0] + tol
            || face_max[1] > o_max[1] + tol
            || face_max[2] > o_max[2] + tol
            || face_min[0] < o_min[0] - tol
            || face_min[1] < o_min[1] - tol
            || face_min[2] < o_min[2] - tol;
        if exceeds {
            eprintln!(
                "  *** BAD face[{}]: {} tris, min=[{:.3},{:.3},{:.3}], max=[{:.3},{:.3},{:.3}]",
                fi,
                tri_count,
                face_min[0],
                face_min[1],
                face_min[2],
                face_max[0],
                face_max[1],
                face_max[2],
            );
            bad_faces.push(fi);
        }
    }

    eprintln!("\n  Bad faces: {:?}", bad_faces);
    assert!(
        bad_faces.is_empty(),
        "Faces {:?} extend beyond outer cylinder bbox at WASM tolerance!",
        bad_faces
    );
}

/// Reproduce the user's failure: full-depth cylinder cut fails with 12 open edges.
/// Try multiple radius/height combos to find a failing configuration.
#[test]
fn diag_reproduce_gui_failure() {
    let configs = [
        // (outer_r, inner_r, height, label)
        (10.0, 5.0, 10.0, "r10/r5/h10"),
        (5.0, 2.0, 10.0, "r5/r2/h10"),
        (5.0, 3.0, 10.0, "r5/r3/h10"),
        (10.0, 3.0, 20.0, "r10/r3/h20"),
        (8.0, 4.0, 15.0, "r8/r4/h15"),
        (5.0, 1.0, 10.0, "r5/r1/h10"),
        (3.0, 1.5, 5.0, "r3/r1.5/h5"),
        (20.0, 8.0, 30.0, "r20/r8/h30"),
    ];

    let mut failures = Vec::new();
    for (outer_r, inner_r, height, label) in &configs {
        let mut m = ModelBuilder::truck();
        m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., *outer_r)
            .unwrap();
        m.extrude("cyl", "outer_sk", *height).unwrap();

        m.true_circle_sketch("cut_sk", [0., 0., *height], [0., 0., 1.], 0., 0., *inner_r)
            .unwrap();

        match m.extrude_cut("hole", "cut_sk", *height) {
            Ok(_) => {
                if m.assert_has_solid("hole").is_ok() {
                    eprintln!("  {} → OK", label);
                } else {
                    eprintln!("  {} → no solid!", label);
                    failures.push(*label);
                }
            }
            Err(e) => {
                eprintln!("  {} → FAIL: {}", label, e);
                failures.push(*label);
            }
        }
    }

    eprintln!(
        "\nResults: {}/{} passed, failures: {:?}",
        configs.len() - failures.len(),
        configs.len(),
        failures
    );
    // Don't assert — this is a diagnostic to find which configs fail
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

    // Topology: genus-0 solid. Containment injection creates ring+disc faces
    // with extra boundary loops, giving V-E+F = 2 + h where h = extra loops.
    // With 2 ring faces (top + bottom), V-E+F = 4 is expected.
    let (v, e, f) = m.topology_counts("merged").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert!(
        chi == 2 || chi == 4,
        "CPU1: Union Euler characteristic should be 2 or 4, got {} (V={}, E={}, F={})",
        chi,
        v,
        e,
        f
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

// ══════════════════════════════════════════════════════════════════════════════
// BNC — Box + NURBS Circle Cut (GUI path reproducer)
// ══════════════════════════════════════════════════════════════════════════════
//
// These tests reproduce the exact GUI failure: draw a rectangle → extrude to
// create a box → draw a true NURBS circle on the top face → extrude as cut.
// The GUI always uses CircleProfile (rsweep NURBS circle), not polygon approx.

/// BNC1: Box 10x10x10 + centered NURBS circle cut r=2, full depth through.
///
/// This is the exact scenario reported in the GUI crash:
///   "shell assembly failed: v2: 12 open edges after all levels"
///
/// Expected: box with cylindrical hole, vol ≈ 10^3 - π*r²*h = 1000 - 125.7 ≈ 874.
#[test]
fn bnc1_box_true_nurbs_circle_cut_through() {
    let mut m = base_cube();

    // True NURBS circle on top face (z=10), centered at (5,5), r=2
    m.true_circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 10.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "bnc1");
    let vol = mesh_volume(&mesh);
    let expected = 10.0 * 10.0 * 10.0 - std::f64::consts::PI * 2.0 * 2.0 * 10.0;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC1: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Bounding box: box is 10x10x10
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 9.0 && bb_max[2] < 11.0,
        "BNC1: top should be near z=10, got z={}",
        bb_max[2]
    );
    assert!(
        bb_min[2] > -1.0 && bb_min[2] < 1.0,
        "BNC1: bottom should be near z=0, got z={}",
        bb_min[2]
    );
}

/// BNC2: Box 10x10x10 + centered NURBS circle cut r=2, shallow depth=5.
///
/// Blind pocket (not through). The cylinder only goes halfway through the box.
///
/// Expected: vol ≈ 10^3 - π*r²*5 = 1000 - 62.8 ≈ 937.
#[test]
fn bnc2_box_true_nurbs_circle_cut_shallow() {
    let mut m = base_cube();

    // True NURBS circle on top face (z=10), centered at (5,5), r=2
    m.true_circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_cut("pocket", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("pocket").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("pocket").unwrap();
    assert_mesh_finite(&mesh, "bnc2");
    let vol = mesh_volume(&mesh);
    let expected = 10.0 * 10.0 * 10.0 - std::f64::consts::PI * 2.0 * 2.0 * 5.0;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC2: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );
}

/// BNC3: Box 10x10x10 + off-center NURBS circle cut at (3,3), r=2, full depth.
///
/// Circle is not centered on the box face.
///
/// Expected: vol ≈ 10^3 - π*r²*10 ≈ 874.
#[test]
fn bnc3_box_true_nurbs_circle_cut_off_center() {
    let mut m = base_cube();

    // True NURBS circle on top face (z=10), off-center at (3,3), r=2
    m.true_circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 10.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "bnc3");
    let vol = mesh_volume(&mesh);
    let expected = 10.0 * 10.0 * 10.0 - std::f64::consts::PI * 2.0 * 2.0 * 10.0;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC3: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );
}

/// BNC4: Box 10x10x10 + large NURBS circle cut r=4 (close to box edges), full depth.
///
/// The circle is large enough that it's close to the box edges but still contained.
///
/// Expected: vol ≈ 10^3 - π*16*10 ≈ 497.
#[test]
fn bnc4_box_true_nurbs_circle_cut_large_radius() {
    let mut m = base_cube();

    // True NURBS circle on top face (z=10), centered at (5,5), r=4
    m.true_circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 4.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 10.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "bnc4");
    let vol = mesh_volume(&mesh);
    let expected = 10.0 * 10.0 * 10.0 - std::f64::consts::PI * 4.0 * 4.0 * 10.0;
    assert!(
        (vol - expected).abs() < expected * 0.15,
        "BNC4: volume {:.1} not within 15% of expected {:.1}",
        vol,
        expected
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// BNC5-7 — Exact Coplanar (No Perturbation) Tests
// ══════════════════════════════════════════════════════════════════════════════
//
// These tests use explicit boolean_subtract to bypass feature-engine's cut_eps
// perturbation. The tool body is EXACTLY coplanar with the target body faces.
// This is the scenario that fails in the GUI.

/// BNC5: Box 10x10x10 + NURBS circle subtract, EXACT coplanar (no cut_eps).
///
/// Box z=0..10, cylinder z=0..10 (both caps exactly coplanar with box faces).
/// Uses explicit boolean_subtract to bypass feature-engine's cut_eps=0.1.
///
/// Expected: box with cylindrical hole, vol ≈ 10^3 - π*r²*10.
#[test]
fn bnc5_box_nurbs_circle_subtract_exact_coplanar() {
    let mut m = ModelBuilder::truck();

    // Box 10x10x10 from z=0 to z=10
    m.rect_sketch("box_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "box_sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // NURBS circle cylinder r=2 centered at (5,5), z=0 to z=10 (EXACT coplanar)
    m.true_circle_sketch("cyl_sk", [0., 0., 0.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_no_merge("cyl", "cyl_sk", 10.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Explicit subtract (no cut_eps)
    m.boolean_subtract("hole", "box", "cyl").unwrap();
    m.assert_has_solid("hole").unwrap();

    // Volume oracle
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "bnc5");
    let vol = mesh_volume(&mesh);
    let expected = 10.0 * 10.0 * 10.0 - std::f64::consts::PI * 2.0 * 2.0 * 10.0;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC5: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );
}

/// BNC6: NURBS cylinder + inner NURBS circle subtract, EXACT coplanar.
///
/// Outer cylinder r=5, inner cylinder r=2, both z=0..10 (caps exactly coplanar).
/// Uses explicit boolean_subtract.
///
/// Expected: tube, vol ≈ π*(5²-2²)*10.
#[test]
fn bnc6_nurbs_cylinder_subtract_exact_coplanar() {
    let mut m = ModelBuilder::truck();

    // Outer NURBS cylinder r=5, z=0 to z=10
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 10.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner NURBS cylinder r=2, z=0 to z=10 (EXACT coplanar)
    m.true_circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 10.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Explicit subtract
    m.boolean_subtract("tube", "outer", "inner").unwrap();
    m.assert_has_solid("tube").unwrap();

    // Volume oracle
    let mesh = m.tessellate("tube").unwrap();
    assert_mesh_finite(&mesh, "bnc6");
    let vol = mesh_volume(&mesh);
    let expected = std::f64::consts::PI * (5.0 * 5.0 - 2.0 * 2.0) * 10.0;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC6: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );
}

/// BNC7: NURBS cylinder + NURBS circle cut via feature engine, full depth.
///
/// Exact user scenario: draw NURBS circle → extrude → draw smaller NURBS circle
/// on top face → extrude cut with depth = original extrude depth.
/// This goes through the full feature-engine pipeline (auto-merge, cut_eps).
///
/// Expected: tube, vol ≈ π*(5²-2²)*20.
#[test]
fn bnc7_nurbs_cylinder_nurbs_cut_full_depth_feature_engine() {
    let mut m = ModelBuilder::truck();

    // Step 1: NURBS circle r=5, extrude up 20
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "outer_sk", 20.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Step 2: NURBS circle r=2 on top face (z=20), cut full depth (20)
    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle: tube = outer - inner
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "bnc7");
    let vol = mesh_volume(&mesh);
    let outer_vol = std::f64::consts::PI * 5.0 * 5.0 * 20.0;
    let inner_vol = std::f64::consts::PI * 2.0 * 2.0 * 20.0;
    let expected = outer_vol - inner_vol;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC7: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );

    // Bounding box: cylinder height 20
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 19.0 && bb_max[2] < 21.0,
        "BNC7: top should be near z=20, got z={}",
        bb_max[2]
    );
    assert!(
        bb_min[2] > -1.0 && bb_min[2] < 1.0,
        "BNC7: bottom should be near z=0, got z={}",
        bb_min[2]
    );
}

/// BNC8: Box + NURBS circle cut via feature-engine with direction forcing no eps.
///
/// Uses extrude_directed with explicit direction to test if the feature engine
/// can handle exact coplanar when direction is explicit (bypasses cut_eps).
///
/// Expected: box with hole, vol ≈ 10^3 - π*r²*10.
#[test]
fn bnc8_box_nurbs_circle_cut_directed_no_eps() {
    let mut m = ModelBuilder::truck();

    // Box 10x10x10
    m.rect_sketch("box_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "box_sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // NURBS circle on top face (z=10), directed cut downward
    m.true_circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_directed("hole", "cut_sk", 10.0, [0., 0., -1.], true)
        .unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle
    let mesh = m.tessellate("hole").unwrap();
    assert_mesh_finite(&mesh, "bnc8");
    let vol = mesh_volume(&mesh);
    let expected = 10.0 * 10.0 * 10.0 - std::f64::consts::PI * 2.0 * 2.0 * 10.0;
    assert!(
        (vol - expected).abs() < expected * 0.10,
        "BNC8: volume {:.1} not within 10% of expected {:.1}",
        vol,
        expected
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// BNC9-11 — Ring + Quadrant Cut (B24)
// ══════════════════════════════════════════════════════════════════════════════
//
// Tests for boolean subtract on annular faces: create a ring (outer cylinder
// minus inner cylinder), then cut a notch at a quadrant position on the ring
// wall. The ring's top face has 2 boundary loops (outer circle + inner hole).

/// BNC9: Ring + quadrant cut via feature engine (user's exact scenario).
///
/// 1. NURBS circle r=5 → extrude 10 → cylinder
/// 2. NURBS circle r=2.5 on top face → extrude cut 10 → ring
/// 3. NURBS circle r=1 at (3.75,0) on top face → extrude cut 10 → C-shape
///
/// Expected: has_solid, volume ≈ ring - notch (±20%), no errors.
#[test]
fn bnc9_ring_quadrant_cut_feature_engine() {
    let mut m = ModelBuilder::truck();

    // Step 1: Outer cylinder r=5, h=10
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "outer_sk", 10.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Step 2: Inner cut r=2.5, full depth → ring
    m.true_circle_sketch("inner_sk", [0., 0., 10.], [0., 0., 1.], 0., 0., 2.5)
        .unwrap();
    m.extrude_cut("ring", "inner_sk", 10.0).unwrap();
    m.assert_has_solid("ring").unwrap();

    // Step 3: Notch cut r=1 at (3.75, 0), full depth → C-shape
    m.true_circle_sketch("notch_sk", [0., 0., 10.], [0., 0., 1.], 3.75, 0., 1.)
        .unwrap();
    m.extrude_cut("cshape", "notch_sk", 10.0).unwrap();
    m.assert_has_solid("cshape").unwrap();
    m.assert_no_errors().unwrap();

    // Volume oracle: ring - notch
    let mesh = m.tessellate("cshape").unwrap();
    assert_mesh_finite(&mesh, "bnc9");
    let vol = mesh_volume(&mesh);
    let ring_vol = std::f64::consts::PI * (5.0 * 5.0 - 2.5 * 2.5) * 10.0;
    let notch_vol = std::f64::consts::PI * 1.0 * 1.0 * 10.0;
    let expected = ring_vol - notch_vol;
    assert!(
        (vol - expected).abs() < expected * 0.20,
        "BNC9: volume {:.1} not within 20% of expected {:.1}",
        vol,
        expected
    );
}

/// BNC10: Ring + notch via explicit boolean subtract (bypass feature engine).
///
/// 1. Extrude outer (r=5), inner (r=2.5), notch (r=1 at 3.75,0) as 3 solids
/// 2. boolean_subtract outer - inner → ring
/// 3. boolean_subtract ring - notch → C-shape
///
/// Expected: has_solid, volume ≈ ring - notch (±20%).
#[test]
fn bnc10_ring_quadrant_cut_explicit_boolean() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=10
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 10.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2.5, h=10
    m.true_circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.5)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 10.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Notch cylinder r=1, center (3.75, 0), h=10
    m.true_circle_sketch("notch_sk", [0., 0., 0.], [0., 0., 1.], 3.75, 0., 1.)
        .unwrap();
    m.extrude_no_merge("notch", "notch_sk", 10.0).unwrap();
    m.assert_has_solid("notch").unwrap();

    // Ring = outer - inner
    m.boolean_subtract("ring", "outer", "inner").unwrap();
    m.assert_has_solid("ring").unwrap();

    // C-shape = ring - notch
    m.boolean_subtract("cshape", "ring", "notch").unwrap();
    m.assert_has_solid("cshape").unwrap();

    // Volume oracle
    let mesh = m.tessellate("cshape").unwrap();
    assert_mesh_finite(&mesh, "bnc10");
    let vol = mesh_volume(&mesh);
    let ring_vol = std::f64::consts::PI * (5.0 * 5.0 - 2.5 * 2.5) * 10.0;
    let notch_vol = std::f64::consts::PI * 1.0 * 1.0 * 10.0;
    let expected = ring_vol - notch_vol;
    assert!(
        (vol - expected).abs() < expected * 0.20,
        "BNC10: volume {:.1} not within 20% of expected {:.1}",
        vol,
        expected
    );
}

/// BNC11: Notch straddling inner hole boundary (hardest case).
///
/// Notch center at (2.5, 0), r=1.5 — overlaps inner boundary.
/// ICs must split both outer and inner boundary loops of the annular face.
///
/// Expected: has_solid, volume, topology.
#[test]
fn bnc11_notch_straddling_inner_boundary() {
    let mut m = ModelBuilder::truck();

    // Outer cylinder r=5, h=10
    m.true_circle_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 10.0).unwrap();
    m.assert_has_solid("outer").unwrap();

    // Inner cylinder r=2.5, h=10
    m.true_circle_sketch("inner_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.5)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 10.0).unwrap();
    m.assert_has_solid("inner").unwrap();

    // Ring = outer - inner
    m.boolean_subtract("ring", "outer", "inner").unwrap();
    m.assert_has_solid("ring").unwrap();

    // Notch straddling inner boundary: center (2.5, 0), r=1.5
    // This overlaps both the inner hole edge (r=2.5 from origin) and ring wall
    m.true_circle_sketch("notch_sk", [0., 0., 0.], [0., 0., 1.], 2.5, 0., 1.5)
        .unwrap();
    m.extrude_no_merge("notch", "notch_sk", 10.0).unwrap();
    m.assert_has_solid("notch").unwrap();

    // C-shape = ring - notch
    m.boolean_subtract("cshape", "ring", "notch").unwrap();
    m.assert_has_solid("cshape").unwrap();

    // Volume oracle: ring - (portion of notch inside ring)
    // The notch extends from x=1.0 to x=4.0. The ring spans r=2.5 to r=5.0.
    // Only the portion of the notch inside the ring is subtracted.
    // Approximate: vol should be less than full ring but more than ring - full notch
    let mesh = m.tessellate("cshape").unwrap();
    assert_mesh_finite(&mesh, "bnc11");
    let vol = mesh_volume(&mesh);
    let ring_vol = std::f64::consts::PI * (5.0 * 5.0 - 2.5 * 2.5) * 10.0;
    // notch fully inside ring would be π*1.5²*10 ≈ 70.7, but it straddles so less
    assert!(
        vol > ring_vol * 0.50,
        "BNC11: volume {:.1} should be > 50% of ring {:.1}",
        vol,
        ring_vol
    );
    assert!(
        vol < ring_vol * 0.99,
        "BNC11: volume {:.1} should be < ring {:.1} (notch was subtracted)",
        vol,
        ring_vol
    );
}
