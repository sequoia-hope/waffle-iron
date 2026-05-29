//! M1 ADVERSARIAL audit (Adversary role, FIP role-separated TDD).
//!
//! Spec: `specs/yang_m1_stage1_orientation.md`. DoD §1.
//!
//! These tests attempt to BREAK the M1 Stage-1 orientation feature and
//! audit it against the Definition of Done. Every test here has real
//! structural assertions. A test that FAILS is a defect finding — it is
//! left in place (per Adversary rules) so the Implementer can see it.
//!
//! Attack map:
//! - A1: tolerance dimensionality (degenerate-face guard) — `t1_*`
//! - A2: branch coverage B1 (keep) and dot==0 — `t2_*`
//! - A4: determinism (I5) — `t4_*`
//!
//! (A3 inputcheck-harness robustness lives in
//! `crates/cherchi-sidecar-rs/tests/inputcheck_adversarial.rs`.)

use cad_primitives::{Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn line(start: u32, end: u32) -> BRepEdge {
    BRepEdge {
        start,
        end,
        curve: Curve::LineSegment,
    }
}

/// Build a single square face of side `s` in the z=0 plane with the given
/// CCW (viewed from +z) vertex order and the given analytic outward normal.
/// Vertices: (0,0,0),(s,0,0),(s,s,0),(0,s,0).
fn square_face(s: f64, vert_order: [u32; 4], normal: Vector3) -> Result<BRep, YangError> {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(s, 0.0, 0.0),
        },
        BRepVertex {
            point: p(s, s, 0.0),
        },
        BRepVertex {
            point: p(0.0, s, 0.0),
        },
    ];
    let edges = vec![
        line(vert_order[0], vert_order[1]),
        line(vert_order[1], vert_order[2]),
        line(vert_order[2], vert_order[3]),
        line(vert_order[3], vert_order[0]),
    ];
    let faces = vec![BRepFace {
        surface: Surface::Plane { normal, d: 0.0 },
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: Vec::new(),
    }];
    BRep::new(verts, edges, faces)
}

// =========================================================================
// A1 — TOLERANCE DIMENSIONALITY (highest priority)
//
// The B3 guard is `newell_magnitude < MIN_FEATURE_SIZE` where
// MIN_FEATURE_SIZE = 1e-6 (a LENGTH). But the Newell vector magnitude is
// 2 * polygon AREA (units length^2). For a square of side s, magnitude =
// 2*s^2, so the guard rejects whenever s < sqrt(5e-7) ≈ 7.07e-4. A square
// of side 1e-4 has area 1e-8 — its side is 100x the 1µm MIN_FEATURE_SIZE
// (so it is a perfectly legal feature as a length), yet it is wrongly
// rejected as DegenerateFace. This is a dimensionally-inconsistent guard.
// =========================================================================

/// A valid, non-degenerate square of side 1e-4 m. Its smallest dimension
/// (1e-4) is 100x MIN_FEATURE_SIZE (1e-6) — a legal feature. It must NOT
/// be rejected as a degenerate face.
///
/// **RED against current (buggy) code** — documents the false-positive
/// degeneracy bug. Newell magnitude = 2*(1e-4)^2 = 2e-8. The buggy guard
/// compares against MIN_FEATURE_SIZE = 1e-6 (a *length*) → 2e-8 < 1e-6 →
/// wrongly rejected. Under the CORRECTED *area* threshold
/// MIN_FEATURE_SIZE^2 = 1e-12 (m^2), 2e-8 > 1e-12 → correctly accepted.
/// Passes once the one-line fix lands.
#[test]
fn t1_small_but_valid_square_is_not_degenerate() {
    // side 1e-4: every edge is 1e-4 m = 100 * MIN_FEATURE_SIZE. Geometrically
    // a legal, well-formed, non-degenerate face.
    let s = 1e-4;
    let result = square_face(s, [0, 1, 2, 3], Vector3::new(0.0, 0.0, 1.0));
    match result {
        Ok(b) => {
            // A genuine square fan-triangulates to 2 tris.
            assert_eq!(
                b.num_tris(),
                2,
                "valid side-1e-4 square should produce 2 triangles"
            );
        }
        Err(YangError::DegenerateFace { face }) => panic!(
            "BUG (tolerance dimensionality): a valid square of side {s} (area {area:.1e}, \
             every edge = {edge_mult}x MIN_FEATURE_SIZE) was wrongly rejected as \
             DegenerateFace {{ face: {face} }}. The guard compares Newell magnitude \
             (2*area = {newell:.1e}, units length^2) against MIN_FEATURE_SIZE \
             (1e-6, units length) — dimensionally wrong. Corrected guard: compare \
             against the AREA threshold MIN_FEATURE_SIZE^2 = 1e-12 m^2, under which \
             {newell:.1e} >= 1e-12 so this face is correctly non-degenerate.",
            s = s,
            area = s * s,
            edge_mult = s / 1e-6,
            newell = 2.0 * s * s,
            face = face,
        ),
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

/// The CORRECT degeneracy boundary under the area threshold: a face is
/// degenerate iff Newell magnitude < MIN_FEATURE_SIZE^2 = 1e-12. For a
/// square, magnitude = 2*s^2, so the boundary side is 2*s^2 = 1e-12 ->
/// s = sqrt(5e-13) ≈ 7.0710678e-7 m.
///
/// A square just ABOVE that boundary (s = 1e-6, magnitude 2e-12 > 1e-12)
/// must be accepted; one well BELOW (s = 1e-7, magnitude 2e-14 < 1e-12)
/// must be rejected. Against current (buggy) production the s=1e-6 case
/// wrongly errors (2e-12 < 1e-6) — an additional RED signal. After the
/// fix, both assertions hold.
#[test]
fn t1_correct_area_threshold_boundary() {
    // boundary: 2*s^2 = 1e-12  ->  s = sqrt(5e-13) ≈ 7.0710678e-7
    // Above boundary → NOT degenerate (accepted under corrected guard).
    let above = 1e-6; // magnitude 2e-12 > 1e-12
    let r_above = square_face(above, [0, 1, 2, 3], Vector3::new(0.0, 0.0, 1.0));
    assert!(
        r_above.is_ok(),
        "square of side {above:.1e} (Newell mag {:.1e}) is ABOVE the corrected \
         area threshold 1e-12 and must be accepted; got {r_above:?}",
        2.0 * above * above
    );

    // Well below boundary → degenerate (rejected even under corrected guard).
    let below = 1e-7; // magnitude 2e-14 < 1e-12
    let r_below = square_face(below, [0, 1, 2, 3], Vector3::new(0.0, 0.0, 1.0));
    assert!(
        matches!(r_below, Err(YangError::DegenerateFace { .. })),
        "square of side {below:.1e} (Newell mag {:.1e}) is BELOW the corrected \
         area threshold 1e-12 and must be rejected as degenerate; got {r_below:?}",
        2.0 * below * below
    );
}

/// A genuine sub-feature-area sliver (long edges, vanishing area) MUST be
/// caught even under the CORRECTED area threshold (1e-12). Triangle
/// (0,0,0),(1,0,0),(0,1e-13,0): edges ~1 m long, area = 0.5*1*1e-13 =
/// 5e-14. Newell magnitude = 2*area = 1e-13 < 1e-12 → degenerate. This
/// holds the line that the dimensionality fix does NOT over-loosen: a face
/// with area below MIN_FEATURE_SIZE^2 is still rejected.
///
/// (Note: the previous version used h=1e-9, area ~5e-10, magnitude ~1e-9 —
/// that is ABOVE the corrected 1e-12 threshold, so it would NO LONGER be
/// degenerate under the fix. This geometry is chosen sub-1e-12 on purpose.)
#[test]
fn t1_genuine_sliver_triangle_is_caught() {
    let h = 1e-13;
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, h, 0.0),
        },
    ];
    let edges = vec![line(0, 1), line(1, 2), line(2, 0)];
    let faces = vec![BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
    }];
    let r = BRep::new(verts, edges, faces);
    assert!(
        matches!(r, Err(YangError::DegenerateFace { face: 0 })),
        "a sliver with area {:.2e} (Newell mag {:.2e} < 1e-12) must be caught \
         as degenerate even under the corrected area threshold; got {r:?}",
        0.5 * h,
        1.0 * h
    );
}

// =========================================================================
// A2 — BRANCH COVERAGE (P4 / DoD §1.2)
//
// Branch table: B1 (dot>0 keep), B2 (dot<0 flip), B3 (degenerate).
// B2 is exercised by the existing cube/tetra fixtures (loops are flipped).
// B3 by the existing degenerate test. B1 (a loop ALREADY correctly wound,
// so winding MUST be preserved) is NOT independently asserted by the M1
// suite — this fills that gap. We also probe the dot==0 boundary.
// =========================================================================

/// B1: a single square face whose loop order [0,1,2,3] is ALREADY CCW
/// (agrees with the +z outward normal). The fan must NOT be flipped — the
/// output triangle normals must keep agreeing with +z. Guards against a
/// double-flip / always-flip regression. Should PASS on correct code.
#[test]
fn t2_b1_already_correct_winding_is_preserved() {
    // [0,1,2,3] = (0,0)->(1,0)->(1,1)->(0,1): CCW from +z. Newell = +2*z.
    // dot(Newell, +z) = +2 > 0 → B1 keep.
    let b = square_face(1.0, [0, 1, 2, 3], Vector3::new(0.0, 0.0, 1.0)).unwrap();
    let mesh = b.as_mesh();
    assert_eq!(mesh.tris.len(), 2);
    for &t in &mesh.tris {
        let a = mesh.verts[t[0] as usize];
        let bb = mesh.verts[t[1] as usize];
        let c = mesh.verts[t[2] as usize];
        // geometric normal = (b-a) x (c-a)
        let u = [bb.x() - a.x(), bb.y() - a.y(), bb.z() - a.z()];
        let v = [c.x() - a.x(), c.y() - a.y(), c.z() - a.z()];
        let nz = u[0] * v[1] - u[1] * v[0]; // z component of cross
        assert!(
            nz > 0.0,
            "B1: already-CCW loop must NOT be flipped; tri {t:?} z-normal {nz} should be > 0"
        );
    }
}

/// B1 vs B2 must differ materially (DoD §1.3 "if a branch can be inverted
/// and tests still pass, the feature is not done"). Same geometry, opposite
/// stated normals → the two output meshes must have OPPOSITE winding.
#[test]
fn t2_b1_and_b2_produce_opposite_winding() {
    let up = square_face(1.0, [0, 1, 2, 3], Vector3::new(0.0, 0.0, 1.0)).unwrap();
    let down = square_face(1.0, [0, 1, 2, 3], Vector3::new(0.0, 0.0, -1.0)).unwrap();
    // First triangle of each. Same vertex set, but winding must be reversed.
    let t_up = up.as_mesh().tris[0];
    let t_down = down.as_mesh().tris[0];
    assert_ne!(
        t_up, t_down,
        "B1 (keep) and B2 (flip) must produce materially different winding; \
         got identical {t_up:?} — orientation branch is inert"
    );
    // Specifically a reversal: [a,b,c] vs [a,c,b]-style opposite orientation.
    let z = |b: &BRep, t: [u32; 3]| {
        let m = b.as_mesh();
        let a = m.verts[t[0] as usize];
        let bb = m.verts[t[1] as usize];
        let c = m.verts[t[2] as usize];
        let u = [bb.x() - a.x(), bb.y() - a.y(), bb.z() - a.z()];
        let v = [c.x() - a.x(), c.y() - a.y(), c.z() - a.z()];
        u[0] * v[1] - u[1] * v[0]
    };
    assert!(z(&up, t_up) > 0.0, "B1 mesh winds +z");
    assert!(z(&down, t_down) < 0.0, "B2 mesh winds -z");
}

/// dot == 0 boundary: a stated normal PERPENDICULAR to the actual face
/// plane (face lies in z=0 but stated normal is +x). The Newell normal is
/// ±z, so dot(Newell, +x) = 0, which is NOT < 0 → the loop is kept (B1
/// path taken by default). This documents the implementation's behavior at
/// the undefined dot==0 boundary: it does NOT error, it keeps the loop.
///
/// This is a spec gap: the branch table only defines dot>0, dot<0, and
/// ‖N‖<MIN_FEATURE_SIZE. dot==0 with a non-degenerate face (inconsistent
/// stated normal) falls through to "keep", silently. Reported as a
/// documentation/spec gap, not a hard failure. Passes on current code.
#[test]
fn t2_dot_zero_inconsistent_normal_keeps_loop_no_error() {
    // Face in z=0 plane, but stated normal +x (perpendicular to true plane).
    let r = square_face(1.0, [0, 1, 2, 3], Vector3::new(1.0, 0.0, 0.0));
    match r {
        Ok(b) => {
            assert_eq!(
                b.num_tris(),
                2,
                "dot==0 (inconsistent normal) currently falls through to B1-keep \
                 and still tessellates (no error). This is an undocumented branch \
                 outcome — the spec branch table does not cover dot==0 for a \
                 non-degenerate face."
            );
        }
        Err(e) => panic!(
            "dot==0 inconsistent-normal case unexpectedly errored: {e:?} — \
             update the spec branch table to cover this outcome"
        ),
    }
}

// =========================================================================
// A4 — DETERMINISM (I5 / DoD §1.3)
// =========================================================================

/// Same input twice → byte-identical mesh (verts in same order, tris in
/// same order). No hashing/ordering nondeterminism in Stage 1.
#[test]
fn t4_brep_new_is_deterministic() {
    let build = || {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 1.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            },
        ];
        let face_verts: [[u32; 4]; 6] = [
            [0, 1, 2, 3],
            [4, 7, 6, 5],
            [0, 4, 5, 1],
            [1, 5, 6, 2],
            [2, 6, 7, 3],
            [3, 7, 4, 0],
        ];
        let normals = [
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
        ];
        let mut edges = Vec::new();
        let mut faces = Vec::new();
        for (i, vs) in face_verts.iter().enumerate() {
            let base = edges.len() as u32;
            for k in 0..4 {
                edges.push(line(vs[k], vs[(k + 1) % 4]));
            }
            faces.push(BRepFace {
                surface: Surface::Plane {
                    normal: normals[i],
                    d: 0.0,
                },
                outer_loop: vec![base, base + 1, base + 2, base + 3],
                inner_loops: Vec::new(),
            });
        }
        BRep::new(verts, edges, faces).unwrap()
    };

    let b1 = build();
    let b2 = build();
    assert_eq!(
        b1.as_mesh().verts,
        b2.as_mesh().verts,
        "verts must be byte-identical across builds"
    );
    assert_eq!(
        b1.as_mesh().tris,
        b2.as_mesh().tris,
        "tris must be byte-identical (same order) across builds"
    );
}
